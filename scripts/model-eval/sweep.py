#!/usr/bin/env python3
"""Controlled parameter sweep: what a launch flag actually changes, if anything.

Reads a declarative experiment plan in TOML, varies one pilotable llama-server
parameter at a time against a frozen baseline, and reports for every level
either a quantified effect with its dispersion or an explicit "no detectable
effect". Records follow the measurement contract in Part 1 of README.md.

Three things here are deliberate and are the difference between an experiment
and a table of numbers.

**The probes are not reimplemented.** `speed_probe.py`, `prefill_curve_probe.py`
and `toolcall_probe.py` are invoked as subprocesses, unmodified, through the
environment contract they already read. A comparison against the frozen baseline
means something only if the code that produced both sides is the same code. A
second producer of the same measurement is exactly the drift the contract
exists to prevent.

**The order is randomised and recorded.** Configurations are shuffled with a
seed carried in the dataset, so thermal drift on a machine that has been running
for two hours shows up as noise spread across the factors instead of as a fake
effect on whichever factor happened to run last.

**The noise floor is measured between runs, not within one.** The baseline is
repeated several times at random positions, and the floor is the dispersion of
those baseline medians. A cv computed over five consecutive requests against an
already-running server contains no server restart and no thermal variance, so it
understates the floor for a comparison between configurations that each required
a restart. Both figures are reported, and the between-run one decides.

Cold is obtained with `cache_prompt: false` on the request. Restarting the
server does not produce a cold request: a fresh server still serves 534 cached
tokens off the chat template prefix. `POST /slots/{id}?action=erase` answers 501
unless the server was launched with `--slot-save-path`. Whatever was attempted,
`cache_state` is read back from `timings.cache_n` and the record states what was
achieved.

Usage:
  sweep.py <plan.toml> [--dry-run] [--out DIR] [--calibrate-from DATASET]
  sweep.py --report <dataset.json>
  sweep.py --ceiling-check [<campaign.json> ...]

Exit codes: 0 clean, 1 one or more configurations failed, 2 plan or usage error.

Stdlib only. Requires Python 3.11 for `tomllib`.
"""

import argparse
import collections
import glob
import json
import math
import os
import random
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    sys.path.insert(0, HERE)

import harness  # noqa: E402
import roofline  # noqa: E402

try:
    import tomllib
except ImportError:  # pragma: no cover - the message is the whole point
    sys.stderr.write(
        "sweep.py needs tomllib, which is stdlib from Python 3.11. Running it "
        "under an older interpreter would mean adding a TOML dependency, and "
        "every dependency in this repository is an ADR.\n"
    )
    raise SystemExit(2)


class PlanError(Exception):
    """A plan that cannot be executed as written."""


class RunFailure(Exception):
    """A configuration that did not produce a measurement, with its reason."""


HOST = "127.0.0.1"
HEALTH_POLL_S = 1.0

# Applied to the request seconds read from a reference campaign, to cover prompt
# tokenisation and probe process startup, neither of which appears in
# `request_wall_ms`. Calibrated against the first sweep dataset for the same
# label when one exists, which is why `--calibrate-from` prefers a sweep over a
# baseline campaign.
DEFAULT_TOKENISATION_FACTOR = 1.6
DEFAULT_PROBE_STARTUP_S = 5.0
DEFAULT_MODEL_LOAD_S = 60.0
DEFAULT_TOOLCALL_S = 90.0

SUBPROCESS_PROBES = {
    "speed": "speed_probe.py",
    "prefill_curve": "prefill_curve_probe.py",
    "warm_continuation": "cache_reuse_probe.py",
}
KNOWN_PROBES = set(SUBPROCESS_PROBES) | {"concurrency"}

# `warm_continuation` runs first, and the order is load-bearing rather than a
# preference. 1.12 finding 1: a single request of exactly `4 + n_ubatch + 1`
# tokens leaves a checkpoint the invalidation loop can never erase, and every
# continuation for the rest of the process then recomputes its whole prompt. The
# prefill curve issues such a request at its 512-token point under the default
# `-ub`, and the speed probe can issue one at other `-ub` levels. A warm
# continuation measured after either would be measuring whether the trap fired,
# at a magnitude that swamps any launch flag. Running it against a server that
# has answered nothing is the only way its numbers mean what they say.
#
# The rest is cheapest first, so a configuration that is going to fail fails
# before the prefill curve has spent four minutes on it.
PROBE_ORDER = ("warm_continuation", "speed", "concurrency", "prefill_curve")

# Mirrors LlamaServerConfig in crates/apollia-runtime/src/llama_server/config.rs.
# `None` means the flag is omitted and the engine default applies, exactly as on
# the Rust side, so a plan that says nothing about `-ub` measures the engine's
# own default rather than a value this script invented.
CONFIG_FIELDS = (
    "n_ctx",
    "n_gpu_layers",
    "n_batch",
    "n_ubatch",
    "n_parallel",
    "cont_batching",
    "cache_type_k",
    "cache_type_v",
    "cache_reuse",
    "ctx_checkpoints",
    "checkpoint_min_step",
    "flash_attn",
    "metrics",
)
# Launch parameters with no field in LlamaServerConfig. The runtime can still
# reach them, through `extra_args`, which Rust appends last; it cannot set them
# as a named field, and a result here that argued for changing one would argue
# first for adding the field. 2.5 records the divergence rather than hiding it.
NOT_IN_RUST = ("ctx_checkpoints", "checkpoint_min_step")
# Reported in the record's `engine` block. `n_ctx` and `n_parallel` are excluded:
# the probes read those back from `/props` as the server actually applied them,
# and an observation outranks a declaration.
ENGINE_REPORTED = (
    "n_gpu_layers",
    "n_batch",
    "n_ubatch",
    "cont_batching",
    "cache_type_k",
    "cache_type_v",
    "cache_reuse",
    "ctx_checkpoints",
    "checkpoint_min_step",
    "flash_attn",
)

BASELINE_DEFAULTS = {
    "n_ctx": 32768,
    "n_gpu_layers": 999,
    "n_batch": None,
    "n_ubatch": None,
    "n_parallel": 1,
    "cont_batching": True,
    "cache_type_k": None,
    "cache_type_v": None,
    "cache_reuse": None,
    "ctx_checkpoints": None,
    "checkpoint_min_step": None,
    "flash_attn": "on",
    "metrics": False,
}


# ---------------------------------------------------------------------------
# Launch arguments, mirroring the Rust build_args
# ---------------------------------------------------------------------------


def build_args(config, port):
    """The argv, in the same order the runtime emits it.

    Field for field and order for order with `build_args` in
    `crates/apollia-runtime/src/llama_server/config.rs`, so a record's
    `launch_args` and a `llama.server.spawn.config` log line are comparable
    without a mapping table. A sweep that launched the engine differently from
    the product would measure a configuration nobody ships.
    """
    args = ["-m", str(config["model_path"])]
    args += ["-ngl", str(config["n_gpu_layers"])]
    args += ["-c", str(config["n_ctx"])]
    if config.get("n_batch") is not None:
        args += ["-b", str(config["n_batch"])]
    if config.get("n_ubatch") is not None:
        args += ["-ub", str(config["n_ubatch"])]
    if config.get("cache_type_k") is not None:
        args += ["-ctk", str(config["cache_type_k"])]
    if config.get("cache_type_v") is not None:
        args += ["-ctv", str(config["cache_type_v"])]
    if config.get("cache_reuse") is not None:
        args += ["--cache-reuse", str(config["cache_reuse"])]
    # No counterpart in the Rust builder, so there is no order to mirror. They
    # sit here, immediately after the other cache flags, and `NOT_IN_RUST` is
    # what the report reads to say so.
    if config.get("ctx_checkpoints") is not None:
        args += ["-ctxcp", str(config["ctx_checkpoints"])]
    if config.get("checkpoint_min_step") is not None:
        args += ["-cms", str(config["checkpoint_min_step"])]
    if config.get("n_parallel") is not None:
        args += ["-np", str(config["n_parallel"])]
    if config.get("cont_batching") is not None:
        args.append("-cb" if config["cont_batching"] else "-nocb")
    if config.get("flash_attn") is not None:
        args += ["--flash-attn", str(config["flash_attn"])]
    args += ["--jinja", "--reasoning-format", "none"]
    if config.get("metrics"):
        args.append("--metrics")
    args += ["--host", HOST, "--port", str(port)]
    return args


def engine_extra(config):
    """The launch fields that belong in a record's `engine` block, 1.6."""
    return {name: config.get(name) for name in ENGINE_REPORTED}


# ---------------------------------------------------------------------------
# Plan
# ---------------------------------------------------------------------------

PLAN_KEYS = {
    "name",
    "seed",
    "repetitions",
    "baseline_repeats",
    "probes",
    "max_tokens",
    "prompt_tok",
    "lengths",
    "page_cache_first_load",
    "tokenisation_factor",
    "continuation_prompt_tok",
    "continuation_suffix_tok",
    "continuation_max_tokens",
}
MODEL_KEYS = {"label", "path"}
ENGINE_KEYS = {"binary", "port", "health_timeout_s", "model_load_s"}
FACTOR_KEYS = {
    "name",
    "field",
    "fields",
    "levels",
    "probes",
    "hold_slot_ctx",
    "quality_gate",
    "note",
}


def _reject_unknown(where, mapping, allowed):
    unknown = sorted(set(mapping) - allowed)
    if unknown:
        raise PlanError(
            "%s carries unknown key(s) %s. A misspelled key that is silently "
            "ignored produces a campaign that looks complete and measured "
            "nothing, so it is an error here." % (where, ", ".join(unknown))
        )


# What each launch parameter may hold. Validating names without validating
# values lets `flash_attn = true` through, which reaches the command line as
# `--flash-attn True` and fails at spawn, which is one wasted model load per
# affected level in a campaign meant to run unattended.
CONFIG_TYPES = {
    "n_ctx": ("integer", lambda v: isinstance(v, int) and not isinstance(v, bool) and v > 0),
    "n_gpu_layers": ("integer", lambda v: isinstance(v, int) and not isinstance(v, bool)),
    "n_batch": ("positive integer", lambda v: isinstance(v, int) and not isinstance(v, bool) and v > 0),
    "n_ubatch": ("positive integer", lambda v: isinstance(v, int) and not isinstance(v, bool) and v > 0),
    "n_parallel": ("positive integer", lambda v: isinstance(v, int) and not isinstance(v, bool) and v > 0),
    "cont_batching": ("boolean", lambda v: isinstance(v, bool)),
    "cache_type_k": ("cache type name", lambda v: isinstance(v, str) and v in CACHE_TYPES),
    "cache_type_v": ("cache type name", lambda v: isinstance(v, str) and v in CACHE_TYPES),
    "cache_reuse": ("integer at or above 0", lambda v: isinstance(v, int) and not isinstance(v, bool) and v >= 0),
    "ctx_checkpoints": ("integer at or above 0", lambda v: isinstance(v, int) and not isinstance(v, bool) and v >= 0),
    "checkpoint_min_step": ("integer at or above 0", lambda v: isinstance(v, int) and not isinstance(v, bool) and v >= 0),
    "flash_attn": ("one of on, off, auto", lambda v: v in FLASH_ATTN_MODES),
    "metrics": ("boolean", lambda v: isinstance(v, bool)),
}
# The engine's own list, from `llama-server --help`. A name outside it is
# rejected here rather than after a model load.
CACHE_TYPES = ("f32", "f16", "bf16", "q8_0", "q4_0", "q4_1", "iq4_nl", "q5_0", "q5_1")
FLASH_ATTN_MODES = ("on", "off", "auto")


def validate_config_values(config, where):
    for field, value in config.items():
        if field == "model_path" or value is None:
            continue
        expected, ok = CONFIG_TYPES.get(field, (None, None))
        if ok is None or ok(value):
            continue
        raise PlanError(
            "%s %s is %r, expected %s. It would reach the command line verbatim."
            % (where, field, value, expected)
        )


def load_plan(path):
    with open(path, "rb") as handle:
        raw = tomllib.load(handle)

    _reject_unknown(
        "the plan file",
        raw,
        {"plan", "model", "engine", "baseline", "factor", "interaction"},
    )
    for section in ("plan", "model", "engine", "baseline"):
        if section not in raw:
            raise PlanError("the plan has no [%s] section" % section)

    _reject_unknown("[plan]", raw["plan"], PLAN_KEYS)
    _reject_unknown("[model]", raw["model"], MODEL_KEYS)
    _reject_unknown("[engine]", raw["engine"], ENGINE_KEYS)
    _reject_unknown("[baseline]", raw["baseline"], set(CONFIG_FIELDS))

    plan = dict(raw["plan"])
    plan.setdefault("seed", 0)
    plan.setdefault("repetitions", 5)
    plan.setdefault("baseline_repeats", 5)
    plan.setdefault("probes", ["speed", "prefill_curve"])
    plan.setdefault("max_tokens", 200)
    plan.setdefault("prompt_tok", 2048)
    plan.setdefault("lengths", [512, 1024, 2048, 4096, 8192, 16384, 32768])
    plan.setdefault("page_cache_first_load", "unknown")
    plan.setdefault("tokenisation_factor", DEFAULT_TOKENISATION_FACTOR)
    # The ReAct shape of 1.12 finding 1: a 4096-token prefix plus a 64-token
    # appended result, 4214 tokens measured. Held at the same values so the
    # grid's warm figures sit directly beside that table instead of needing to
    # be reconciled with it.
    plan.setdefault("continuation_prompt_tok", 4096)
    plan.setdefault("continuation_suffix_tok", 64)
    plan.setdefault("continuation_max_tokens", 32)
    if "name" not in plan:
        raise PlanError("[plan] has no name")
    if plan["page_cache_first_load"] not in harness.PAGE_CACHE_STATES:
        raise PlanError(
            "[plan] page_cache_first_load is %r, expected one of %s"
            % (plan["page_cache_first_load"], ", ".join(harness.PAGE_CACHE_STATES))
        )
    if plan["baseline_repeats"] < 2:
        raise PlanError(
            "[plan] baseline_repeats is %d. The noise floor is the dispersion "
            "of the baseline medians across runs, and a single run has none."
            % plan["baseline_repeats"]
        )
    for name in plan["probes"]:
        if name not in KNOWN_PROBES:
            raise PlanError(
                "[plan] probes names %r, known probes are %s"
                % (name, ", ".join(sorted(KNOWN_PROBES)))
            )

    # The prefill curve is compared per length, and two points are taken to
    # describe the same length when their measured token counts sit within
    # CURVE_MATCH_LOG_RATIO. Targets spaced closer than that would be merged into
    # one comparison without anyone being told, so they are rejected here instead.
    ladder = sorted(int(n) for n in plan["lengths"])
    for lower, upper in zip(ladder, ladder[1:]):
        if math.log(upper / float(lower)) <= CURVE_MATCH_LOG_RATIO:
            raise PlanError(
                "[plan] lengths %d and %d are within the %.2f log-ratio the curve "
                "comparison uses to decide that two points describe the same "
                "length. They would silently become one row. Space them further "
                "apart, or the curve reports fewer lengths than it measured."
                % (lower, upper, CURVE_MATCH_LOG_RATIO)
            )

    model = dict(raw["model"])
    for key in ("label", "path"):
        if key not in model:
            raise PlanError("[model] has no %s" % key)
    model["path"] = os.path.expanduser(model["path"])

    engine = dict(raw["engine"])
    engine.setdefault("binary", "")
    engine.setdefault("port", 8090)
    engine.setdefault("health_timeout_s", 300)
    engine.setdefault("model_load_s", DEFAULT_MODEL_LOAD_S)
    engine["binary"] = roofline.locate_llama_server(engine["binary"] or None)

    baseline = dict(BASELINE_DEFAULTS)
    baseline.update(raw["baseline"])
    validate_config_values(baseline, "[baseline]")
    baseline["model_path"] = model["path"]

    factors = []
    for entry in raw.get("factor", []):
        _reject_unknown("a [[factor]]", entry, FACTOR_KEYS)
        factors.append(_load_factor(entry, baseline, plan))

    names = [f["name"] for f in factors]
    if len(names) != len(set(names)):
        raise PlanError("two factors share a name: %s" % ", ".join(sorted(names)))

    interactions = [
        _load_interaction(entry, baseline, plan, factors)
        for entry in raw.get("interaction", [])
    ]

    return {
        "path": os.path.abspath(path),
        "plan": plan,
        "model": model,
        "engine": engine,
        "baseline": baseline,
        "factors": factors,
        "interactions": interactions,
    }


INTERACTION_KEYS = {"name", "fields", "cells", "probes", "note"}


def _load_interaction(entry, baseline, plan, factors):
    """A deliberately combined configuration, run after the single factors.

    The experiment-design rule is one factor at a time against the frozen
    baseline first, and combinations only afterwards. This is where that rule
    graduates rather than where it is broken: an interaction is declared
    separately, executed after nothing else changes about how a run is made, and
    reported in its own section so no reader can mistake a two-flag cell for a
    one-flag verdict.
    """
    _reject_unknown("an [[interaction]]", entry, INTERACTION_KEYS)
    name = entry.get("name")
    if not name:
        raise PlanError("an [[interaction]] has no name")
    fields = list(entry.get("fields") or [])
    if len(fields) < 2:
        raise PlanError(
            "interaction %s names %d field(s). An interaction is two or more "
            "flags moved together; one flag is a factor." % (name, len(fields))
        )
    for field in fields:
        if field not in CONFIG_FIELDS:
            raise PlanError(
                "interaction %s varies %r, which is not a launch parameter"
                % (name, field)
            )
    cells = entry.get("cells")
    if not cells:
        raise PlanError("interaction %s has no cells" % name)
    normalised = []
    for cell in cells:
        values = list(cell) if isinstance(cell, list) else [cell]
        if len(values) != len(fields):
            raise PlanError(
                "interaction %s: cell %r has %d value(s) for %d field(s)"
                % (name, cell, len(values), len(fields))
            )
        validate_config_values(dict(zip(fields, values)), "interaction %s" % name)
        normalised.append(values)

    # Not enforced, because a null result on an interaction is a real answer and
    # the rule is about ordering rather than permission. Recorded so the report
    # can say whether the precondition held.
    swept_individually = sorted(
        field for field in fields if any(field in f["fields"] for f in factors)
    )

    return {
        "name": name,
        "fields": fields,
        "cells": normalised,
        "probes": ordered_probes(entry.get("probes") or plan["probes"]),
        "note": entry.get("note"),
        "baseline_cell": [baseline.get(f) for f in fields],
        "fields_swept_individually": swept_individually,
    }


def _load_factor(entry, baseline, plan):
    if "name" not in entry:
        raise PlanError("a [[factor]] has no name")
    name = entry["name"]
    if "field" in entry and "fields" in entry:
        raise PlanError("factor %s declares both field and fields" % name)
    if "field" in entry:
        fields = [entry["field"]]
    elif "fields" in entry:
        fields = list(entry["fields"])
    else:
        raise PlanError("factor %s declares neither field nor fields" % name)
    for field in fields:
        if field not in CONFIG_FIELDS:
            raise PlanError(
                "factor %s varies %r, which is not a launch parameter. Known: %s"
                % (name, field, ", ".join(CONFIG_FIELDS))
            )
        # `cache_reuse` was excluded while the reuse collapse was an
        # uncontrolled switch of the same magnitude as any effect a sweep could
        # find. 1.12 finding 1 established the mechanism and named the single
        # request that triggers it, and the warm continuation probe now runs
        # first in every run so nothing precedes it in the slot. The exclusion
        # was spent by that work, not waived.

    levels = entry.get("levels")
    if not levels:
        raise PlanError("factor %s has no levels" % name)
    normalised = []
    for level in levels:
        values = list(level) if isinstance(level, list) else [level]
        if len(values) != len(fields):
            raise PlanError(
                "factor %s: level %r has %d value(s) for %d field(s)"
                % (name, level, len(values), len(fields))
            )
        validate_config_values(dict(zip(fields, values)), "factor %s" % name)
        normalised.append(values)

    probes = list(entry.get("probes") or plan["probes"])
    for probe in probes:
        if probe not in KNOWN_PROBES:
            raise PlanError(
                "factor %s names probe %r, known probes are %s"
                % (name, probe, ", ".join(sorted(KNOWN_PROBES)))
            )

    hold_slot_ctx = bool(entry.get("hold_slot_ctx", False))
    if hold_slot_ctx and fields != ["n_parallel"]:
        raise PlanError(
            "factor %s sets hold_slot_ctx but varies %s. Holding slot capacity "
            "constant means scaling n_ctx with the slot count, which only has a "
            "meaning for n_parallel." % (name, ", ".join(fields))
        )

    gate = entry.get("quality_gate")
    if gate not in (None, "toolcall"):
        raise PlanError(
            "factor %s declares quality_gate %r, only 'toolcall' is implemented"
            % (name, gate)
        )

    return {
        "name": name,
        "fields": fields,
        "levels": normalised,
        "probes": probes,
        "hold_slot_ctx": hold_slot_ctx,
        "quality_gate": gate,
        "note": entry.get("note"),
        "baseline_level": [baseline.get(f) for f in fields],
    }


def level_label(values):
    return "/".join("default" if v is None else str(v) for v in values)


def ordered_probes(names):
    return [name for name in PROBE_ORDER if name in set(names)]


# ---------------------------------------------------------------------------
# Run list
# ---------------------------------------------------------------------------


def build_runs(spec):
    """Baseline repeats plus one run per factor level, in randomised order."""
    plan = spec["plan"]
    baseline = spec["baseline"]

    baseline_probes = set(plan["probes"])
    wants_toolcall = False
    for factor in spec["factors"]:
        baseline_probes.update(factor["probes"])
        wants_toolcall = wants_toolcall or factor["quality_gate"] == "toolcall"
    for interaction in spec.get("interactions") or []:
        baseline_probes.update(interaction["probes"])
    # Every metric a factor produces needs a baseline to be compared against,
    # including the ones only that factor's probe emits. A concurrency figure
    # with nothing at one slot to sit beside is a number, not a comparison.
    baseline_probes = ordered_probes(baseline_probes)

    runs = []
    for _ in range(plan["baseline_repeats"]):
        runs.append(
            {
                "role": "baseline",
                "factor": None,
                "level": None,
                "config": dict(baseline),
                "probes": list(baseline_probes),
                "quality_gate": "toolcall" if wants_toolcall else None,
                "note": None,
            }
        )

    for factor in spec["factors"]:
        for values in factor["levels"]:
            config = dict(baseline)
            for field, value in zip(factor["fields"], values):
                config[field] = value
            note = factor["note"]
            if factor["hold_slot_ctx"]:
                config["n_ctx"] = baseline["n_ctx"] * int(values[0])
                note = (
                    "n_ctx scaled to %d so each of the %d slots keeps %d tokens: "
                    "llama-server divides -c across slots, so raising -np at a "
                    "fixed -c would conflate slot count with slot capacity. Two "
                    "flags move, the controlled quantity is held."
                    % (config["n_ctx"], int(values[0]), baseline["n_ctx"])
                ) + (("; " + note) if note else "")
            runs.append(
                {
                    "role": "level",
                    "factor": factor["name"],
                    "level": level_label(values),
                    "config": config,
                    "probes": ordered_probes(factor["probes"]),
                    "quality_gate": factor["quality_gate"],
                    "note": note,
                }
            )

    for interaction in spec.get("interactions") or []:
        for values in interaction["cells"]:
            config = dict(baseline)
            for field, value in zip(interaction["fields"], values):
                config[field] = value
            runs.append(
                {
                    "role": "interaction",
                    "factor": interaction["name"],
                    "level": level_label(values),
                    "config": config,
                    "probes": list(interaction["probes"]),
                    "quality_gate": None,
                    "note": interaction["note"],
                }
            )

    # Every run is shuffled together, single factors and interactions alike.
    # Holding the interactions back to the end of the campaign would give them a
    # systematically warmer machine than the factors they are read against,
    # which is the confound the randomised order exists to remove. Ordering the
    # analysis is a reporting concern and is handled there.
    random.Random(plan["seed"]).shuffle(runs)
    for index, run in enumerate(runs):
        run["run_index"] = index
        run["concurrency"] = int(run["config"].get("n_parallel") or 1)
    return runs


# ---------------------------------------------------------------------------
# Server lifecycle
# ---------------------------------------------------------------------------


def port_is_free(port, timeout=2.0):
    """False when anything at all answers on this port.

    A leftover llama-server holding the port would serve every request in the
    sweep from a model and a configuration nobody recorded, and every number
    would look plausible. This is checked before the first spawn rather than
    discovered in the data.

    An HTTP error status counts as occupied. A server still loading its model
    answers `/health` with 503, and treating that as free would be the worst
    case of all: the port is taken by something that is about to start
    answering normally.
    """
    try:
        urllib.request.urlopen(
            "http://%s:%d/health" % (HOST, port), timeout=timeout
        ).read()
        return False
    except urllib.error.HTTPError:
        return False
    except (urllib.error.URLError, OSError):
        return True


class Server(object):
    """One llama-server process, with its log kept for the failure path."""

    def __init__(self, binary, args, port, log_path, health_timeout_s):
        self.binary = binary
        self.args = args
        self.port = port
        self.log_path = log_path
        self.health_timeout_s = health_timeout_s
        self.process = None
        self.load_seconds = None

    @property
    def base_url(self):
        return "http://%s:%d/v1" % (HOST, self.port)

    def tail(self, lines=15):
        try:
            with open(self.log_path, "r", encoding="utf-8", errors="replace") as handle:
                return "".join(handle.readlines()[-lines:]).rstrip()
        except OSError:
            return "(no server log)"

    def start(self):
        started = time.monotonic()
        handle = open(self.log_path, "w", encoding="utf-8")
        try:
            self.process = subprocess.Popen(
                [self.binary] + self.args, stdout=handle, stderr=subprocess.STDOUT
            )
        except OSError as exc:
            handle.close()
            raise RunFailure("could not spawn %s: %s" % (self.binary, exc))
        finally:
            handle.close()

        deadline = started + self.health_timeout_s
        while time.monotonic() < deadline:
            if self.process.poll() is not None:
                raise RunFailure(
                    "llama-server exited with code %d during load:\n%s"
                    % (self.process.returncode, self.tail())
                )
            try:
                urllib.request.urlopen(
                    "http://%s:%d/health" % (HOST, self.port), timeout=5
                ).read()
                self.load_seconds = time.monotonic() - started
                return
            except (urllib.error.URLError, OSError):
                time.sleep(HEALTH_POLL_S)
        self.stop()
        raise RunFailure(
            "llama-server was not healthy within %ds:\n%s"
            % (self.health_timeout_s, self.tail())
        )

    def stop(self):
        if self.process is None:
            return
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=30)
            except subprocess.TimeoutExpired:
                self.process.kill()
                try:
                    self.process.wait(timeout=30)
                except subprocess.TimeoutExpired:
                    pass
        self.process = None


# ---------------------------------------------------------------------------
# Probes
# ---------------------------------------------------------------------------


def probe_env(spec, run, server, sha, page_cache, probe=None):
    plan = spec["plan"]
    env = dict(os.environ)
    env.update(
        {
            "BASE_URL": server.base_url,
            "MODEL": spec["model"]["label"],
            "LABEL": spec["model"]["label"],
            "MODEL_PATH": spec["model"]["path"],
            "LLAMA_BIN": spec["engine"]["binary"],
            "LAUNCH_ARGS": json.dumps([server.binary] + server.args),
            "N_CTX": str(run["config"]["n_ctx"]),
            "REPS": str(plan["repetitions"]),
            "MAX_TOKENS": str(plan["max_tokens"]),
            "PROMPT_TOK": str(plan["prompt_tok"]),
            "LENGTHS": ",".join(str(n) for n in plan["lengths"]),
            "RUN_INDEX": str(run["run_index"]),
            "RUN_ORDER": "randomised",
            "PAGE_CACHE": page_cache,
            "ENGINE_EXTRA": json.dumps(engine_extra(run["config"])),
            "MODEL_SHA256": sha or "",
        }
    )
    if probe == "warm_continuation":
        # A different shape from the speed probe's, and deliberately so: this
        # one measures a continuation, which is a prefix plus an appended
        # result, not a prompt resent unchanged.
        env["PROMPT_TOK"] = str(plan["continuation_prompt_tok"])
        env["SUFFIX_TOK"] = str(plan["continuation_suffix_tok"])
        env["MAX_TOKENS"] = str(plan["continuation_max_tokens"])
    return env


def run_subprocess_probe(name, spec, run, server, sha, page_cache, scratch):
    """Invoke an existing probe unchanged and take back its records."""
    script = os.path.join(HERE, SUBPROCESS_PROBES[name])
    out = os.path.join(scratch, "run%02d-%s.json" % (run["run_index"], name))
    env = probe_env(spec, run, server, sha, page_cache, probe=name)
    env["OUT"] = out
    env["CAMPAIGN_ID"] = "%s-run%02d" % (spec["plan"]["name"], run["run_index"])
    result = subprocess.run(
        [sys.executable, script],
        env=env,
        cwd=HERE,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise RunFailure(
            "%s exited %d: %s"
            % (name, result.returncode, (result.stderr or "").strip()[-400:])
        )
    try:
        return harness.read_campaign(out)["records"]
    except (OSError, ValueError) as exc:
        raise RunFailure("%s wrote no readable campaign: %s" % (name, exc))


def run_toolcall_gate(spec, run, server, sha, page_cache, scratch):
    """The quality gate: the scored tool-calling probe, wrapped as a record.

    `harness._is_degenerate` catches a model that has collapsed into repetition.
    It does not catch a model that has merely got worse, which is precisely what
    KV quantisation does, so a scored probe decides whether a faster
    configuration is allowed to be called an improvement.
    """
    env = probe_env(spec, run, server, sha, page_cache)
    scored = subprocess.run(
        [sys.executable, os.path.join(HERE, "toolcall_probe.py")],
        env=env,
        cwd=HERE,
        capture_output=True,
        text=True,
    )
    if scored.returncode != 0:
        raise RunFailure(
            "toolcall probe exited %d: %s"
            % (scored.returncode, (scored.stderr or "").strip()[-400:])
        )

    out = os.path.join(scratch, "run%02d-toolcall.json" % run["run_index"])
    env["QUALITY_OUT"] = out
    env["RECORD_SUFFIX"] = ""
    env["CAMPAIGN_ID"] = "%s-run%02d" % (spec["plan"]["name"], run["run_index"])
    wrapped = subprocess.run(
        [
            sys.executable,
            os.path.join(HERE, "quality_record.py"),
            spec["model"]["label"],
            scored.stdout.strip(),
            "",
            "",
        ],
        env=env,
        cwd=HERE,
        capture_output=True,
        text=True,
    )
    if wrapped.returncode != 0:
        raise RunFailure(
            "quality_record exited %d: %s"
            % (wrapped.returncode, (wrapped.stderr or "").strip()[-400:])
        )
    return harness.read_campaign(out)["records"]


def run_concurrency_probe(spec, run, server, sha, page_cache):
    """N concurrent cold requests, repeated, at the configuration's slot count.

    A single sequential client cannot measure what `-np` does. It occupies one
    slot and leaves the rest idle, so the flag would be reported as having no
    effect, which is true of that client and false of the server. Every request
    is cold via `cache_prompt: false` over a distinct prompt, and the round's
    aggregate rate is generated tokens over the round's wall-clock, so a slot
    that finished early does not inflate the figure.
    """
    plan = spec["plan"]
    slots = run["concurrency"]
    reps = plan["repetitions"]
    base_url = server.base_url
    label = spec["model"]["label"]

    props = harness.server_props(base_url)
    slot_tok = props["n_ctx_slot_tok"]
    observed_slots = props.get("total_slots")

    # Tokenisation happens before the clock starts. Calibrating a prompt costs
    # several round trips, and doing it inside the timed region would put the
    # tokeniser in the throughput figure.
    prompts = {}
    for rep in range(reps):
        for slot in range(slots):
            # Collision-free by construction across every (rep, slot). A scheme
            # of the form a*rep + b*slot collides whenever the offsets share a
            # factor with the coefficients, which is exactly the kind of quiet
            # duplicate that would let two slots share a prefix.
            prompts[(rep, slot)] = harness.filler_for_tokens(
                base_url, plan["prompt_tok"], salt=1 + rep * (slots + 1) + slot
            )

    samples = []
    rounds = []
    for rep in range(reps):
        barrier = threading.Barrier(slots)
        results = [None] * slots
        errors = [None] * slots

        def worker(slot, rep=rep):
            try:
                barrier.wait(timeout=120)
                results[slot] = harness.measure(
                    base_url,
                    label,
                    prompts[(rep, slot)],
                    max_tokens=plan["max_tokens"],
                    cache_prompt=False,
                    n_ctx_slot_tok=slot_tok,
                )
            except (harness.ProbeError, threading.BrokenBarrierError, OSError, ValueError) as exc:
                errors[slot] = exc

        threads = [threading.Thread(target=worker, args=(slot,)) for slot in range(slots)]
        started = time.perf_counter()
        for thread in threads:
            thread.start()
        for thread in threads:
            thread.join()
        wall_ms = (time.perf_counter() - started) * 1000.0

        failed = [exc for exc in errors if exc is not None]
        if failed:
            raise RunFailure(
                "concurrency probe: %d of %d requests failed, first: %s"
                % (len(failed), slots, failed[0])
            )
        round_samples = [s for s in results if s is not None]
        samples.extend(round_samples)
        decoded = sum(s["decode_tok"] or 0 for s in round_samples)
        rounds.append(
            {
                "round_wall_ms": wall_ms,
                "aggregate_decode_tps_wall": (decoded / (wall_ms / 1000.0)) if wall_ms else None,
                "decode_tok_total": decoded,
            }
        )

    engine = harness.engine_block(
        run["config"]["n_ctx"],
        slot_tok,
        n_parallel=observed_slots,
        **engine_extra(run["config"])
    )

    # Cold was requested with cache_prompt=false. What was achieved is read back
    # off every sample, exactly as 1.4.4 requires: a probe does not get to call a
    # measurement cold because it intended it to be. A round where one slot came
    # back warm is a different measurement, and the record says so instead of
    # averaging it in under a label it has not earned.
    achieved = sorted({s.get("cache_state") for s in samples if s.get("cache_state")})
    warm_samples = sum(1 for s in samples if s.get("cache_state") != "cold")

    notes = (
        "%d concurrent requests per round, %d rounds, distinct prompt per slot, "
        "cold requested with cache_prompt=false and cache_state read back from "
        "timings.cache_n; achieved %s on %d sample(s), %d not cold. "
        "aggregate_decode_tps_wall is generated tokens over the round's "
        "wall-clock, prefill included, so an early-finishing slot cannot "
        "inflate it. It carries the _wall suffix for the reason I5 gives: it "
        "is not comparable to decode_tps, which excludes prefill and is "
        "reported per slot on the same record"
        % (slots, reps, "/".join(achieved) or "nothing", len(samples), warm_samples)
    )
    if run["note"]:
        notes = notes + "; " + run["note"]

    return [
        harness.build_record(
            probe="concurrency",
            record_id="concurrency-%s" % label,
            label=label,
            provenance_block=harness.provenance(
                spec["model"]["path"],
                [server.binary] + server.args,
                spec["engine"]["binary"],
                model_sha256=sha,
            ),
            conditions_block=harness.conditions(
                run_index=run["run_index"],
                run_order="randomised",
                page_cache=page_cache,
                notes=notes,
            ),
            engine=engine,
            campaign_id="%s-run%02d" % (spec["plan"]["name"], run["run_index"]),
            samples=samples,
            stats=harness.stats_for(samples),
            extra={
                "concurrency": {
                    "n_requests": slots,
                    "rounds": reps,
                    "cache_state_requested": "cold",
                    "cache_state_achieved": achieved,
                    "samples_not_cold": warm_samples,
                    "aggregate_decode_tps_wall": harness.aggregate(
                        [r["aggregate_decode_tps_wall"] for r in rounds]
                    ),
                    "round_wall_ms": harness.aggregate(
                        [r["round_wall_ms"] for r in rounds]
                    ),
                }
            },
        )
    ]


# ---------------------------------------------------------------------------
# Execution
# ---------------------------------------------------------------------------


def defective_samples(record):
    """Samples whose text disqualifies the record's timings, 1.5 and item 5."""
    reasons = []
    for index, sample in enumerate(record.get("samples") or []):
        if sample.get("degenerate"):
            reasons.append("sample %d degenerate" % index)
        if sample.get("empty"):
            reasons.append("sample %d empty" % index)
    return reasons


def execute(spec, runs, out_dir, scratch):
    plan = spec["plan"]
    label = spec["model"]["label"]
    started = harness.now_rfc3339()
    campaign_id = "sweep-%s-%s" % (plan["name"], time.strftime("%Y%m%dT%H%M%SZ", time.gmtime()))
    dataset_path = os.path.join(out_dir, campaign_id + ".json")

    if not os.path.isfile(spec["model"]["path"]):
        raise PlanError("model file not found: %s" % spec["model"]["path"])
    if not port_is_free(spec["engine"]["port"]):
        raise PlanError(
            "something already answers /health on port %d. It would serve every "
            "request in this sweep from a model and a configuration this run "
            "never recorded." % spec["engine"]["port"]
        )

    sys.stdout.write("[sweep] hashing %s once for the whole campaign\n" % os.path.basename(spec["model"]["path"]))
    sys.stdout.flush()
    sha = harness._sha256(spec["model"]["path"])

    records = []
    excluded = []
    timing = {}
    loaded_before = False

    for run in runs:
        page_cache = "warm" if loaded_before else plan["page_cache_first_load"]
        header = "run %02d/%02d  %s" % (
            run["run_index"] + 1,
            len(runs),
            "baseline" if run["role"] == "baseline" else "%s = %s" % (run["factor"], run["level"]),
        )
        sys.stdout.write("[sweep] %s\n" % header)
        sys.stdout.flush()

        args = build_args(run["config"], spec["engine"]["port"])
        server = Server(
            spec["engine"]["binary"],
            args,
            spec["engine"]["port"],
            os.path.join(scratch, "llama-run%02d.log" % run["run_index"]),
            spec["engine"]["health_timeout_s"],
        )
        run_started = time.monotonic()
        produced = []
        try:
            server.start()
            loaded_before = True
            for name in run["probes"]:
                probe_started = time.monotonic()
                if name == "concurrency":
                    produced.extend(
                        run_concurrency_probe(spec, run, server, sha, page_cache)
                    )
                else:
                    produced.extend(
                        run_subprocess_probe(
                            name, spec, run, server, sha, page_cache, scratch
                        )
                    )
                timing.setdefault(name, []).append(time.monotonic() - probe_started)
            if run["quality_gate"] == "toolcall":
                probe_started = time.monotonic()
                produced.extend(
                    run_toolcall_gate(spec, run, server, sha, page_cache, scratch)
                )
                timing.setdefault("toolcall", []).append(time.monotonic() - probe_started)
        except RunFailure as exc:
            sys.stdout.write("[sweep]   FAILED: %s\n" % str(exc).splitlines()[0])
            excluded.append(
                {
                    "record_id": "%s#%02d" % (run["factor"] or "baseline", run["run_index"]),
                    "reason": str(exc),
                    "run_index": run["run_index"],
                    "role": run["role"],
                    "factor": run["factor"],
                    "level": run["level"],
                    "launch_args": [server.binary] + args,
                }
            )
            continue
        finally:
            server.stop()
            if server.load_seconds is not None:
                timing.setdefault("model_load", []).append(server.load_seconds)

        for record in produced:
            record["record_id"] = "%s#%02d" % (record["record_id"], run["run_index"])
            record["campaign_id"] = campaign_id
            record["sweep"] = {
                "plan": plan["name"],
                "role": run["role"],
                "factor": run["factor"],
                "level": run["level"],
                "shuffle_seed": plan["seed"],
                "note": run["note"],
            }
            defects = defective_samples(record)
            if defects:
                # Recorded under `sweep`, not appended to `invalid`. That array
                # holds the identifiers of 1.5, and inventing one would break a
                # consumer that maps an entry to an invariant. Degenerate output
                # is a defect in the measurement, not a violated invariant.
                record["sweep"]["defective"] = defects
        records.extend(produced)
        sys.stdout.write(
            "[sweep]   %d record(s) in %.0fs\n" % (len(produced), time.monotonic() - run_started)
        )
        sys.stdout.flush()

    sweep_block = {
        "plan": plan["name"],
        "plan_path": spec["path"],
        "model_label": label,
        "model_path": spec["model"]["path"],
        "run_order": "randomised",
        "shuffle_seed": plan["seed"],
        "repetitions": plan["repetitions"],
        "baseline_repeats": plan["baseline_repeats"],
        "page_cache_first_load": plan["page_cache_first_load"],
        "runs_planned": len(runs),
        "runs_completed": len(runs) - len(excluded),
        "realised_order": [
            {
                "run_index": r["run_index"],
                "role": r["role"],
                "factor": r["factor"],
                "level": r["level"],
                "launch_args": [spec["engine"]["binary"]] + build_args(r["config"], spec["engine"]["port"]),
            }
            for r in runs
        ],
        "interactions": [
            {
                "name": i["name"],
                "fields": i["fields"],
                "baseline_cell": level_label(i["baseline_cell"]),
                "cells": [level_label(c) for c in i["cells"]],
                "fields_swept_individually": i["fields_swept_individually"],
            }
            for i in spec.get("interactions") or []
        ],
        "factors": [
            {
                "name": f["name"],
                "fields": f["fields"],
                "baseline_level": level_label(f["baseline_level"]),
                "levels": [level_label(v) for v in f["levels"]],
                "quality_gate": f["quality_gate"],
                "hold_slot_ctx": f["hold_slot_ctx"],
            }
            for f in spec["factors"]
        ],
        "observed_seconds": {
            name: (sum(values) / len(values)) for name, values in timing.items() if values
        },
    }

    container = harness.write_campaign(dataset_path, campaign_id, records, started, excluded)
    container["sweep"] = sweep_block
    with open(dataset_path, "w", encoding="utf-8") as handle:
        json.dump(container, handle, ensure_ascii=False, indent=1)
    return dataset_path, container


# ---------------------------------------------------------------------------
# Analysis
# ---------------------------------------------------------------------------


def speed_variant(record):
    """`cold`, `warm`, or None when the record's samples disagree.

    None is not a shrug. Under I2 a record whose samples came back in different
    cache states is two measurements wearing one label, and averaging them would
    mix a prefill rate with a fixed per-request cost. Such a record is excluded
    from every metric, and `unmatched_records` makes the exclusion visible so a
    level cannot disappear from the report without saying why.
    """
    states = {s.get("cache_state") for s in record.get("samples") or []}
    if states == {"cold"}:
        return "cold"
    if states == {"warm"}:
        return "warm"
    return None


# 1.4.8: above 0.10 the measurement is unstable and its median is not reported
# as fact. That applies to a level's own five requests, not only to the baseline.
UNSTABLE_CV = 0.10

# Two-sided 95 percent Student t, by degrees of freedom. Stdlib only, so the
# table is transcribed rather than computed. Beyond 30 the normal value is close
# enough that the difference is far below the dispersion being tested.
T95 = {
    1: 12.706, 2: 4.303, 3: 3.182, 4: 2.776, 5: 2.571, 6: 2.447, 7: 2.365,
    8: 2.306, 9: 2.262, 10: 2.228, 11: 2.201, 12: 2.179, 13: 2.160, 14: 2.145,
    15: 2.131, 16: 2.120, 17: 2.110, 18: 2.101, 19: 2.093, 20: 2.086, 21: 2.080,
    22: 2.074, 23: 2.069, 24: 2.064, 25: 2.060, 26: 2.056, 27: 2.052, 28: 2.048,
    29: 2.045, 30: 2.042,
}
T95_LARGE = 1.960

# Two axes carry verdicts, and every factor is reported against both.
#
# COLD is the shape a benchmark measures: a prompt the engine has never seen.
# WARM is the shape an agentic loop actually runs: the same prefix again with a
# tool result appended. They are different machines. A flag can buy one and sell
# the other, and `-ub` does exactly that, which is why a single merged number
# would be worse than no number.
#
# OTHER holds figures that belong to neither verdict: decode, which no launch
# flag has been shown to move, and the concurrency and quality figures.
AXIS_COLD = "cold prefill"
AXIS_WARM = "warm continuation"
AXIS_OTHER = "other"

# Selecting a record within a probe, and what state that record must have
# achieved for its figures to mean what the metric name says.
#
# Selection is by the producer's own record name, never by observed cache state.
# At `-ctxcp 0` this model serves nothing from cache, so BOTH halves of a pair
# come back cold: the continuation of the ReAct pair, and the speed probe's
# resend of an identical prompt. A selector keyed on cache state reclassifies the
# warm record as a cold one, which duplicates it into the cold metrics and
# deletes it from the warm ones. Verified on run 18 of the warm grid and on
# results/prefix-collapse-l4-ctxcp0.json.
#
# `state_gate` is the state the measurement presupposes, or None when the state
# is itself what is being measured. A resend latency measured on a request the
# engine served cold is not a resend latency, so those metrics are gated. The
# continuation metrics are not: a hit ratio falling to zero is the result, not a
# broken measurement, and gating it would hide the finding it exists to report.
Metric = collections.namedtuple(
    "Metric", "key probe selector path kind lower axis state_gate"
)

# `prefill_tps` from a prefill-curve record is deliberately absent. That record's
# top-level stats pool every sample at every length, and which lengths were
# measured depends on what fitted the slot, so a level that changes the slot
# changes the pool and shows a difference that is a change of x-axis rather than
# a change of rate. The curve is compared per length instead, in `curve_analysis`.
METRICS = (
    Metric("prefill_tps cold", "speed", ("id_contains", "-cold"), ("stats", "prefill_tps"), "aggregate", False, AXIS_COLD, "cold"),
    Metric("ttft_cold_ms", "speed", ("id_contains", "-cold"), ("stats", "ttft_cold_ms"), "aggregate", True, AXIS_COLD, "cold"),
    Metric("continuation hit ratio", "cache_reuse", ("id_contains", "-continuation"), ("stats", "prompt_cache_hit_ratio"), "aggregate", False, AXIS_WARM, None),
    Metric("continuation ttft_ms", "cache_reuse", ("id_contains", "-continuation"), ("stats", "ttft_ms"), "aggregate", True, AXIS_WARM, None),
    Metric("continuation recompute tok", "cache_reuse", ("id_contains", "-continuation"), ("stats", "prompt_tok_computed"), "aggregate", True, AXIS_WARM, None),
    Metric("continuation prefill_ms", "cache_reuse", ("id_contains", "-continuation"), ("stats", "prefill_ms"), "aggregate", True, AXIS_WARM, None),
    Metric("seed prefill_tps", "cache_reuse", ("id_contains", "-seed"), ("stats", "prefill_tps"), "aggregate", False, AXIS_COLD, "cold"),
    Metric("decode_tps cold", "speed", ("id_contains", "-cold"), ("stats", "decode_tps"), "aggregate", False, AXIS_OTHER, "cold"),
    Metric("decode_tps warm", "speed", ("id_contains", "-warm"), ("stats", "decode_tps"), "aggregate", False, AXIS_OTHER, "warm"),
    Metric("resend ttft_ms", "speed", ("id_contains", "-warm"), ("stats", "ttft_warm_ms"), "aggregate", True, AXIS_OTHER, "warm"),
    Metric("aggregate_decode_tps_wall", "concurrency", None, ("concurrency", "aggregate_decode_tps_wall"), "aggregate", False, AXIS_OTHER, "cold"),
    Metric("decode_tps per slot", "concurrency", None, ("stats", "decode_tps"), "aggregate", False, AXIS_OTHER, "cold"),
    Metric("toolcall score", "toolcall", None, ("toolcall", "score"), "scalar", False, AXIS_OTHER, None),
)

# The one metric per verdict axis whose direction decides whether a factor's two
# verdicts agree. Cold prefill throughput against the latency a ReAct iteration
# actually pays.
HEADLINE = {AXIS_COLD: "prefill_tps cold", AXIS_WARM: "continuation ttft_ms"}


def dig(record, path):
    node = record
    for key in path:
        if not isinstance(node, dict) or key not in node:
            return None
        node = node[key]
    return node


def numeric(value):
    return value if isinstance(value, (int, float)) else None


def metric_matches(record, probe, selector):
    if record.get("probe") != probe:
        return False
    if selector is None:
        return True
    kind, value = selector
    if kind == "cache_state":
        return speed_variant(record) == value
    if kind == "id_contains":
        return value in str(record.get("record_id") or "")
    return False


def read_metric(record, path, kind):
    """Value, n, p95 and within-run cv for one metric on one record.

    `n` comes from the metric's own aggregate, never from the producer's
    `invalid` array. A record can satisfy I8 on its `stats` block and carry a
    three-round aggregate elsewhere, and trusting the array would let that
    through.
    """
    node = dig(record, path)
    if kind == "scalar":
        value = numeric(node)
        return None if value is None else {"value": value, "n": 1, "p95": None, "cv": None}
    if not isinstance(node, dict):
        return None
    value = numeric(node.get("median"))
    if value is None:
        return None
    return {
        "value": value,
        "n": node.get("n") or 0,
        "p95": numeric(node.get("p95")),
        "cv": numeric(node.get("cv")),
    }


def invariant_order(identifier):
    """Sort I1 before I9 before I13, which a lexicographic sort does not."""
    digits = "".join(c for c in str(identifier) if c.isdigit())
    return (int(digits) if digits else 0, str(identifier))


def record_disqualifiers(record):
    """Invariant violations that bar a record from a comparison.

    I8 is handled separately: it makes a record provisional, which is a weaker
    statement than disqualified. Everything else in the array is a defect in the
    measurement, and I9 says so outright for speed work. A report that read only
    I8 would present a record with a broken prompt accounting, an unfixed seed
    or a first-token time that started after prefill as a clean result.
    """
    return sorted(
        (i for i in (record.get("invalid") or []) if i != "I8"), key=invariant_order
    )


def dedupe_mismatches(items):
    """One line per record and requested state, not one per metric."""
    seen = {}
    for item in items:
        seen.setdefault((item["record_id"], item["requested"]), item)
    return [seen[k] for k in sorted(seen)]


def collect(container):
    """Group every comparable figure by metric, run role, factor and level."""
    grouped = {}
    unmatched = []
    mismatches = []
    for record in container.get("records") or []:
        sweep = record.get("sweep") or {}
        role = sweep.get("role")
        if role is None:
            continue
        matched_any = False
        for metric in METRICS:
            key = metric.key
            if not metric_matches(record, metric.probe, metric.selector):
                continue
            # Computed before the value is read, and recomputed from the
            # samples rather than trusted from a producer's summary. A gated
            # metric whose value is null on an off-state record, which is what
            # I3 does to `ttft_warm_ms` on a request served cold, would
            # otherwise vanish from the report entirely.
            achieved = [s.get("cache_state") for s in record.get("samples") or []]
            off_state = (
                sum(1 for state in achieved if state != metric.state_gate)
                if metric.state_gate
                else 0
            )
            if off_state:
                mismatches.append(
                    {
                        "record_id": record.get("record_id"),
                        "metric": key,
                        "requested": metric.state_gate,
                        "achieved": sorted({a for a in achieved if a}),
                        "off": off_state,
                        "of": len(achieved),
                        "factor": sweep.get("factor"),
                        "level": sweep.get("level"),
                    }
                )
            read = read_metric(record, metric.path, metric.kind)
            if read is None:
                continue
            matched_any = True
            entry = {
                "value": read["value"],
                "n": read["n"],
                "p95": read["p95"],
                "within_cv": read["cv"],
                "run_index": (record.get("conditions") or {}).get("run_index"),
                "record_id": record.get("record_id"),
                "defective": bool(sweep.get("defective")),
                "disqualifiers": record_disqualifiers(record),
                "provisional": read["n"] < 5,
                "unstable": read["cv"] is not None and read["cv"] > UNSTABLE_CV,
                "role": role,
                "state_gate": metric.state_gate,
                "off_state": off_state,
                "n_samples": len(achieved),
                "achieved_states": sorted({a for a in achieved if a}),
            }
            bucket = grouped.setdefault(key, {"baseline": [], "levels": {}})
            if role == "baseline":
                bucket["baseline"].append(entry)
            else:
                bucket["levels"].setdefault(
                    (sweep.get("factor"), sweep.get("level")), []
                ).append(entry)
        if not matched_any and record.get("probe") in ("speed", "concurrency", "cache_reuse"):
            unmatched.append(
                {
                    "record_id": record.get("record_id"),
                    "probe": record.get("probe"),
                    "factor": sweep.get("factor"),
                    "level": sweep.get("level"),
                    "reason": (
                        "samples disagree on cache_state (%s), so the record is "
                        "two measurements under one label and enters none"
                        % ", ".join(
                            sorted(
                                {
                                    str(s.get("cache_state"))
                                    for s in record.get("samples") or []
                                }
                            )
                        )
                        if record.get("probe") == "speed"
                        else "no readable aggregate on the record"
                    ),
                }
            )
    return grouped, unmatched, dedupe_mismatches(mismatches)


def detection_threshold_pct(values):
    """The smallest relative difference this baseline can actually resolve.

    I11 says a delta below the baseline's own cv is no detectable effect. Taken
    literally that is a one-sigma test, and one sigma is not a detection
    threshold: a level's median carries the same between-run error the cv
    describes, so under a null hypothesis roughly a third of comparisons clear it
    by chance. Across a grid of fifteen levels and six metrics that is dozens of
    invented effects.

    The threshold here is the half-width of a two-sided 95 percent interval on
    the difference between a baseline of `n` runs and a single level run, with
    the level assumed to carry the baseline's dispersion because one run cannot
    estimate its own:

        t(0.975, n - 1) * cv * sqrt(1 + 1/n)

    At n = 5 that is 3.04 times the cv rather than 1.0 times it. Returns the
    threshold and the cv it came from, both as percentages, so the report can
    show the dispersion and the bar it implies side by side.
    """
    block = harness.aggregate(values)
    if not block or block["n"] < 2:
        return None, None
    n = block["n"]
    cv = block["cv"]
    t_crit = T95.get(n - 1, T95_LARGE)
    threshold = t_crit * cv * math.sqrt(1.0 + 1.0 / n)
    return 100.0 * threshold, 100.0 * cv


def verdict(delta_pct, threshold_pct, lower_is_better):
    """I11, with a threshold that accounts for the error in both terms."""
    if threshold_pct is None or delta_pct is None:
        return "no threshold, baseline missing"
    if abs(delta_pct) <= threshold_pct:
        return "no detectable effect"
    better = (delta_pct < 0) if lower_is_better else (delta_pct > 0)
    label = "%s, beyond the %.1f %% threshold" % (
        "better" if better else "worse",
        threshold_pct,
    )
    if threshold_pct == 0.0:
        label += " (threshold is zero, baseline runs identical)"
    return label


def row_verdict(entry, delta_pct, threshold_pct, lower, provisional_reasons):
    """One verdict, by precedence. The strongest disqualification wins.

    Order matters: a record that violates an invariant should not be described
    as unstable, and an unstable one should not be described as an effect. Each
    branch withholds a claim rather than making one.
    """
    if entry["disqualifiers"]:
        return "disqualified, violates %s" % ", ".join(entry["disqualifiers"])
    if entry["defective"]:
        return "DEFECTIVE, output degenerate or empty"
    if entry["off_state"]:
        # 1.4.4: the flag states the intent, the response states the fact, and
        # only the second one goes in the record. A resend latency measured on a
        # request the engine served cold is not a resend latency, and reporting
        # it as one would put the configuration's effect on the cache into a
        # figure that claims to be about speed.
        return (
            "%s requested, %d of %d sample(s) came back %s; not a %s measurement"
            % (
                entry["state_gate"],
                entry["off_state"],
                entry["n_samples"],
                "/".join(entry["achieved_states"]) or "nothing",
                entry["state_gate"],
            )
        )
    reasons = list(provisional_reasons)
    if entry["provisional"]:
        reasons.append("this level's aggregate has n = %d, below 5" % entry["n"])
    if reasons:
        return "provisional under I8, not compared: %s" % reasons[0]
    if entry["unstable"]:
        return (
            "unstable, within-run cv %.2f above the 0.10 of 1.4.8; the median is "
            "not reported as fact" % entry["within_cv"]
        )
    return verdict(delta_pct, threshold_pct, lower)


def curve_verdict(delta_pct, length):
    """One curve verdict, by the same precedence `row_verdict` applies.

    An unstable baseline outranks a small sample, which outranks a comparison,
    because each branch withholds a claim the branch below it would make.
    """
    unstable = length["unstable_baseline"]
    if unstable:
        return "withheld, baseline run(s) %s unstable at cv %s, above the 0.10 " "of 1.4.8" % (
            ", ".join(
                "?" if item["run_index"] is None else str(item["run_index"])
                for item in unstable
            ),
            ", ".join("%.2f" % item["within_cv"] for item in unstable),
        )
    if length["n_runs"] < 5:
        return "provisional under I8, %d baseline point(s)" % length["n_runs"]
    return verdict(delta_pct, length["threshold_pct"], False)


def median_of(values):
    block = harness.aggregate(values)
    return block["median"] if block else None


def analyse(container):
    grouped, unmatched, mismatches = collect(container)
    analysis = {}
    for metric in METRICS:
        key, lower, axis = metric.key, metric.lower, metric.axis
        bucket = grouped.get(key)
        if not bucket or not bucket["baseline"]:
            continue
        baseline_values = [e["value"] for e in bucket["baseline"]]
        baseline_block = harness.aggregate(baseline_values)
        threshold_pct, floor_cv_pct = detection_threshold_pct(baseline_values)

        # 1.4.8 applies to the baseline runs too, and it has to: an unstable
        # baseline run does not merely report an unreliable median, it sets the
        # dispersion every threshold is derived from. One contended run among
        # five inflates the threshold enough to swallow a real effect, and the
        # report would then say "no detectable effect" with total confidence.
        # The run is named rather than dropped: choosing which runs to exclude
        # after seeing the answer is how a sweep talks itself into a result.
        unstable_baseline = [
            entry
            for entry in bucket["baseline"]
            if entry["within_cv"] is not None and entry["within_cv"] > UNSTABLE_CV
        ]

        provisional = []
        if unstable_baseline:
            provisional.append(
                "baseline run(s) %s have within-run cv %s, above the 0.10 of "
                "1.4.8; the dispersion across baseline runs, and every threshold "
                "derived from it, is not usable"
                % (
                    ", ".join(str(e["run_index"]) for e in unstable_baseline),
                    ", ".join("%.2f" % e["within_cv"] for e in unstable_baseline),
                )
            )
        if any(entry["provisional"] for entry in bucket["baseline"]):
            provisional.append(
                "a baseline run's aggregate has n below 5"
            )
        if len(baseline_values) < 5:
            provisional.append(
                "the threshold rests on %d baseline run(s), below 5"
                % len(baseline_values)
            )
        baseline_disqualified = sorted(
            {i for entry in bucket["baseline"] for i in entry["disqualifiers"]},
            key=invariant_order,
        )

        rows = []
        for (factor, level), entries in sorted(bucket["levels"].items()):
            for entry in entries:
                reference = baseline_block["median"]
                delta = (
                    100.0 * (entry["value"] - reference) / reference
                    if reference
                    else None
                )
                text = (
                    "baseline disqualified, violates %s"
                    % ", ".join(baseline_disqualified)
                    if baseline_disqualified
                    else row_verdict(entry, delta, threshold_pct, lower, provisional)
                )
                rows.append(
                    {
                        "factor": factor,
                        "level": level,
                        "role": entry["role"],
                        "value": entry["value"],
                        "p95": entry["p95"],
                        "within_cv": entry["within_cv"],
                        "n": entry["n"],
                        "delta_pct": delta,
                        "verdict": text,
                        "comparable": text.startswith(("better", "worse", "no detectable")),
                        "defective": entry["defective"],
                        "provisional": bool(provisional) or entry["provisional"],
                        "disqualifiers": entry["disqualifiers"],
                    }
                )
        within = [e["within_cv"] for e in bucket["baseline"] if e["within_cv"] is not None]
        analysis[key] = {
            "lower_is_better": lower,
            "axis": axis,
            "baseline": {
                "n_runs": baseline_block["n"],
                "n_per_run": min(e["n"] for e in bucket["baseline"]),
                "median": baseline_block["median"],
                "max_of_run_medians": baseline_block["max"],
                "between_run_cv": (floor_cv_pct / 100.0) if floor_cv_pct is not None else None,
                "within_run_cv_median": median_of(within),
                "disqualifiers": baseline_disqualified,
            },
            "threshold_pct": threshold_pct,
            "provisional": provisional,
            "rows": rows,
        }
    return analysis, unmatched, mismatches


def declared_gates(container):
    """Factor levels the plan said would be quality gated."""
    gated = set()
    for factor in (container.get("sweep") or {}).get("factors") or []:
        if factor.get("quality_gate"):
            for level in factor.get("levels") or []:
                gated.add((factor.get("name"), level))
    return gated


def apply_quality_gate(analysis, container):
    """A level that scored worse cannot be reported as a gain, whatever its speed.

    The score is a single observation, so it is provisional under I8 as a
    measurement. It is still admissible as a gate, because sampling is pinned at
    temperature 0 with a fixed seed, which makes the score deterministic rather
    than noisy: a drop is a different answer to the same question, not a
    different draw from the same distribution. The gate only ever withholds a
    claim, never makes one, so the asymmetry is in the safe direction.

    Returns the levels the plan declared gated for which no comparison could be
    made. A gate that passed, a gate never declared and a gate that silently
    evaporated must not read alike.
    """
    scores = analysis.get("toolcall score")
    covered = set()
    regressed = {}
    if scores:
        for row in scores["rows"]:
            covered.add((row["factor"], row["level"]))
            if row["delta_pct"] is not None and row["delta_pct"] < 0:
                regressed[(row["factor"], row["level"])] = row["delta_pct"]

    for key, block in analysis.items():
        if key == "toolcall score":
            continue
        for row in block["rows"]:
            drop = regressed.get((row["factor"], row["level"]))
            if drop is None:
                continue
            note = "QUALITY REGRESSION, tool score %.1f %%, not a gain" % drop
            # Never overwrite a stronger disqualification. A degenerate output, a
            # violated invariant and a provisional aggregate all say the number
            # should not be read at all, which outranks saying it should not be
            # read as a gain.
            was_plain = row["comparable"]
            row["quality_drop_pct"] = drop
            row["comparable"] = False
            row["verdict"] = note if was_plain else "%s; %s" % (row["verdict"], note)

    return sorted(declared_gates(container) - covered)


# Two curve points describe the same length when their measured token counts are
# within this log-ratio. The nominal targets double, a log-ratio of 0.69, so 0.20
# separates neighbours with room while absorbing the two percent convergence
# tolerance of `filler_for_tokens` and the run-to-run wobble around it.
CURVE_MATCH_LOG_RATIO = 0.20


def curve_points(container):
    """Every prefill curve point, grouped by run role and factor level.

    A point carries the dispersion of its own samples and the index of the run
    that produced it, not only its median. The baseline-instability guard needs
    both: 1.4.8 decides on the within-run cv, and a guard that fires without
    naming the run it fired on tells a reader something is wrong without telling
    them what to repeat.
    """
    grouped = {}
    for record in container.get("records") or []:
        if record.get("probe") != "prefill_curve":
            continue
        if record_disqualifiers(record):
            continue
        sweep = record.get("sweep") or {}
        key = (
            ("baseline", "")
            if sweep.get("role") == "baseline"
            else (sweep.get("factor"), sweep.get("level"))
        )
        run_index = (record.get("conditions") or {}).get("run_index")
        for point in record.get("prefill_curve") or []:
            total = point.get("prompt_tok_total")
            block = (point.get("stats") or {}).get("prefill_tps") or {}
            rate = numeric(block.get("median"))
            if total is None or rate is None:
                continue
            grouped.setdefault(key, {}).setdefault(total, []).append(
                {
                    "rate": rate,
                    "within_cv": numeric(block.get("cv")),
                    "run_index": run_index,
                }
            )
    return grouped


def cluster_lengths(totals):
    """Group measured prompt lengths that describe the same nominal target.

    Five baseline runs measure the same target five times and land on five
    slightly different token counts, because `filler_for_tokens` converges to
    within two percent rather than exactly. Clustering by ratio recovers the
    grouping without needing to know the target, the chat template prefix, or
    the convergence tolerance.
    """
    clusters = []
    for total in sorted(totals):
        if clusters and abs(
            math.log(max(total, 1) / float(max(clusters[-1][-1], 1)))
        ) <= CURVE_MATCH_LOG_RATIO:
            clusters[-1].append(total)
        else:
            clusters.append([total])
    return clusters


def curve_analysis(container):
    """The prefill curve compared per length, each length its own comparison.

    Points are paired on their measured `prompt_tok_total`, not on the nominal
    target that produced them. The chat template prefix adds several hundred
    tokens to every prompt, which at the short end of the curve is a large
    fraction of the target: rounding a measured 1046 back to a nominal power of
    two lands on 1024 rather than the 512 it came from, and the whole axis shifts
    by one.

    Each length gets its own baseline across the baseline runs, its own
    dispersion and its own threshold, because that is the only comparison the
    data supports. A single figure pooled across lengths measures which lengths
    fitted the slot, not how fast prefill ran.

    Each length also carries its own instability guard, for the same reason
    `analyse` carries one and for a while this did not. The guard was written
    into the metric table alone, and on the dataset that motivated it this table
    went on emitting seventy verdicts with nothing checking the baseline they
    were measured against.
    """
    grouped = curve_points(container)
    baseline = grouped.pop(("baseline", ""), None)
    if not baseline:
        return None

    lengths = []
    for cluster in cluster_lengths(baseline.keys()):
        entries = []
        for total in cluster:
            entries.extend(baseline[total])
        per_run = [entry["rate"] for entry in entries]
        threshold_pct, floor_cv_pct = detection_threshold_pct(per_run)
        # 1.4.8 applies to the baseline runs here exactly as it does in
        # `analyse`. An unstable baseline run does not merely report an
        # unreliable median: it sets the dispersion this length's threshold is
        # derived from, and an inflated threshold reports a real effect as none
        # with no sign that anything went wrong. The run is named rather than
        # dropped, because choosing which runs to exclude after seeing the
        # answer is how a sweep talks itself into a result.
        unstable = sorted(
            {
                (entry["run_index"], entry["within_cv"])
                for entry in entries
                if entry["within_cv"] is not None and entry["within_cv"] > UNSTABLE_CV
            },
            key=lambda pair: (pair[0] is None, pair[0]),
        )
        lengths.append(
            {
                "centre": median_of([float(t) for t in cluster]),
                "totals": cluster,
                "median": median_of(per_run),
                "n_runs": len(per_run),
                "threshold_pct": threshold_pct,
                "between_run_cv": (floor_cv_pct / 100.0) if floor_cv_pct is not None else None,
                "unstable_baseline": [
                    {
                        "run_index": run_index,
                        "within_cv": within_cv,
                    }
                    for run_index, within_cv in unstable
                ],
            }
        )

    out = []
    for key in sorted(grouped):
        remaining = dict(grouped[key])
        rows = []
        for length in lengths:
            centre = length["centre"]
            candidates = [
                total
                for total in remaining
                if abs(math.log(max(total, 1) / float(max(centre, 1))))
                <= CURVE_MATCH_LOG_RATIO
            ]
            if not candidates:
                continue
            chosen = min(
                candidates,
                key=lambda t: abs(math.log(max(t, 1) / float(max(centre, 1)))),
            )
            # Consumed, so one measurement cannot satisfy two lengths and be
            # counted twice under two different labels.
            level_rates = [entry["rate"] for entry in remaining.pop(chosen)]
            level = median_of(level_rates)
            base = length["median"]
            if base is None or level is None:
                continue
            delta = 100.0 * (level - base) / base if base else None
            rows.append(
                {
                    "prompt_tok": int(centre),
                    "baseline_tps": base,
                    "level_tps": level,
                    "delta_pct": delta,
                    "n_runs": length["n_runs"],
                    "threshold_pct": length["threshold_pct"],
                    "unstable_baseline": length["unstable_baseline"],
                    "verdict": curve_verdict(delta, length),
                }
            )
        matched_centres = {row["prompt_tok"] for row in rows}
        out.append(
            {
                "factor": key[0],
                "level": key[1],
                "rows": rows,
                "missing_in_level": [
                    int(length["centre"])
                    for length in lengths
                    if int(length["centre"]) not in matched_centres
                ],
                "absent_from_baseline": sorted(remaining),
            }
        )
    return out




# ---------------------------------------------------------------------------
# Estimation and dry run
# ---------------------------------------------------------------------------


def reference_seconds(label, calibrate_from, out_dir):
    """Per-probe seconds, preferring a real sweep over the baseline campaign."""
    if calibrate_from:
        paths = [calibrate_from]
    else:
        paths = sorted(glob.glob(os.path.join(out_dir, "sweep-*.json")), reverse=True)
        paths += sorted(glob.glob(os.path.join(out_dir, "baseline-%s.json" % label)))
    for path in paths:
        try:
            with open(path, "r", encoding="utf-8") as handle:
                container = json.load(handle)
        except (OSError, ValueError):
            continue
        observed = (container.get("sweep") or {}).get("observed_seconds")
        if observed:
            return observed, path, "measured, from a previous sweep"
        seconds = {}
        for record in container.get("records") or []:
            block = (record.get("stats") or {}).get("request_wall_ms")
            if not block:
                continue
            probe = record.get("probe")
            seconds[probe] = seconds.get(probe, 0.0) + block["median"] * block["n"] / 1000.0
        if seconds:
            return seconds, path, "request wall-clock from a baseline campaign"
    return {}, None, "no reference available"


def estimate(spec, runs, out_dir, calibrate_from):
    plan = spec["plan"]
    seconds, source, basis = reference_seconds(
        spec["model"]["label"], calibrate_from, out_dir
    )
    measured = basis.startswith("measured")
    factor = 1.0 if measured else plan["tokenisation_factor"]
    load = seconds.get("model_load") if measured else None
    if load is None:
        load = spec["engine"]["model_load_s"]

    def probe_cost(name, slots):
        if name in seconds:
            base = seconds[name] * factor
        elif name == "concurrency":
            # An upper bound, and knowingly so: this probe's wall-clock depends
            # on the aggregate throughput it exists to measure, so estimating it
            # properly would mean assuming the result. The serialised bound is
            # what can be stated without doing that.
            per_request = seconds.get("speed", 60.0) / 15.0
            base = per_request * plan["repetitions"] * slots * factor
        elif name == "toolcall":
            base = DEFAULT_TOOLCALL_S
        else:
            base = 120.0 * factor
        scale = 1.0
        if not measured and name in seconds:
            scale = plan["repetitions"] / 5.0
        return base * scale + (0.0 if measured else DEFAULT_PROBE_STARTUP_S)

    # The model is hashed once for the whole campaign, and on a 20 GiB file that
    # is not a rounding error. 500 MB/s is a conservative figure for `shasum` on
    # this class of machine; it is an assumption, and it is stated rather than
    # folded silently into the total.
    try:
        hash_s = os.path.getsize(spec["model"]["path"]) / 500e6
    except OSError:
        hash_s = 0.0

    total = hash_s
    per_run = []
    bounded = False
    for run in runs:
        cost = load + sum(probe_cost(name, run["concurrency"]) for name in run["probes"])
        if "concurrency" in run["probes"] and "concurrency" not in seconds:
            bounded = True
        if run["quality_gate"] == "toolcall":
            cost += probe_cost("toolcall", run["concurrency"])
        per_run.append(cost)
        total += cost
    return {
        "total_s": total,
        "per_run_s": per_run,
        "source": source,
        "basis": basis,
        "model_load_s": load,
        "model_hash_s": hash_s,
        "concurrency_is_upper_bound": bounded,
    }


def preflight(spec, runs):
    """Everything checkable without spawning anything."""
    notes = []
    try:
        model = roofline.describe_model(spec["model"]["path"])
    except (roofline.RooflineError, OSError) as exc:
        notes.append(("model", "GGUF header unreadable: %s" % exc))
        return notes

    memory = harness.run_capture(["sysctl", "-n", "hw.memsize"])
    total_memory = int(memory.strip()) if memory else 0
    weights = model["file_bytes"]
    train_ctx = model.get("train_ctx")

    seen = set()
    for run in runs:
        config = run["config"]
        key = (config["n_ctx"], config.get("cache_type_k"), config.get("cache_type_v"))
        if key in seen:
            continue
        seen.add(key)
        kv = roofline.kv_cache_bytes(
            model,
            config["n_ctx"],
            config.get("cache_type_k") or "f16",
            config.get("cache_type_v") or "f16",
        )
        footprint = weights + kv
        share = (100.0 * footprint / total_memory) if total_memory else 0.0
        line = "n_ctx %-7d kv %-9s %7.2f GiB   weights + kv %7.2f GiB   %.0f %% of RAM" % (
            config["n_ctx"],
            "%s/%s" % (config.get("cache_type_k") or "f16", config.get("cache_type_v") or "f16"),
            kv / roofline.GIB,
            footprint / roofline.GIB,
            share,
        )
        if share > 75.0:
            line += "   OVER 75 %, load may fail"
        notes.append(("memory", line))
        if train_ctx and config["n_ctx"] > int(train_ctx):
            notes.append(
                (
                    "train_ctx",
                    "n_ctx %d exceeds the model's trained context of %s: beyond "
                    "that, a level changes output quality and not only speed"
                    % (config["n_ctx"], train_ctx),
                )
            )
    return notes


def render_dry_run(spec, runs, estimation, notes, out):
    plan = spec["plan"]
    out.write("SWEEP PLAN  %s\n" % plan["name"])
    out.write("  file          %s\n" % spec["path"])
    out.write("  model         %s\n" % spec["model"]["label"])
    out.write("                %s\n" % spec["model"]["path"])
    out.write("  engine        %s\n" % spec["engine"]["binary"])
    out.write(
        "  order         randomised, seed %d, %d runs\n" % (plan["seed"], len(runs))
    )
    out.write(
        "  repetitions   %d per probe run, %d baseline runs for the noise floor\n"
        % (plan["repetitions"], plan["baseline_repeats"])
    )
    out.write("  page cache    first load %s, warm thereafter\n\n" % plan["page_cache_first_load"])

    out.write("RUN ORDER\n")
    for run, cost in zip(runs, estimation["per_run_s"]):
        what = (
            "baseline"
            if run["role"] == "baseline"
            else "%s = %s" % (run["factor"], run["level"])
        )
        probes = ",".join(run["probes"]) + (
            "+toolcall" if run["quality_gate"] == "toolcall" else ""
        )
        out.write(
            "  %02d  %-28s %-26s ~%5.1f min\n" % (run["run_index"], what, probes, cost / 60.0)
        )
        out.write(
            "      %s\n"
            % " ".join([spec["engine"]["binary"]] + build_args(run["config"], spec["engine"]["port"]))
        )
    out.write("\n")

    if notes:
        out.write("PREFLIGHT\n")
        for kind, line in notes:
            out.write("  %-10s %s\n" % (kind, line))
        out.write("\n")

    out.write("ESTIMATE\n")
    out.write("  wall-clock    %.1f h  (%.0f min)\n" % (estimation["total_s"] / 3600.0, estimation["total_s"] / 60.0))
    out.write("  model load    %.0f s per run\n" % estimation["model_load_s"])
    out.write(
        "  model hash    %.0f s once, at an assumed 500 MB/s\n" % estimation["model_hash_s"]
    )
    out.write("  basis         %s\n" % estimation["basis"])
    out.write("  source        %s\n" % (estimation["source"] or "none"))
    if estimation.get("concurrency_is_upper_bound"):
        out.write(
            "  The concurrency probe is an upper bound, not an estimate: its\n"
            "  wall-clock depends on the aggregate throughput it is there to\n"
            "  measure, and assuming that would be assuming the result.\n"
        )
    out.write(
        "  An estimate, not a budget. It carries the load time and the request\n"
        "  wall-clock of a previous run and nothing about this machine's state\n"
        "  today. Re-run --dry-run with --calibrate-from a completed sweep to\n"
        "  replace it with a measured figure.\n"
    )


# ---------------------------------------------------------------------------
# Report
# ---------------------------------------------------------------------------


def fmt(value, spec="%.2f"):
    return "-" if value is None else spec % value


def render_report(container, out):
    sweep = container.get("sweep") or {}
    records = container.get("records") or []
    excluded = container.get("records_excluded") or []

    out.write("SWEEP  %s\n" % sweep.get("plan", "unknown"))
    out.write("  model         %s\n" % sweep.get("model_label", "unknown"))
    out.write("  campaign      %s\n" % container.get("campaign_id", "unknown"))
    out.write(
        "  runs          %s planned, %s completed, %d excluded\n"
        % (sweep.get("runs_planned", "?"), sweep.get("runs_completed", "?"), len(excluded))
    )
    out.write(
        "  order         %s, seed %s\n"
        % (sweep.get("run_order", "unknown"), sweep.get("shuffle_seed", "?"))
    )
    out.write(
        "  page cache    first load %s, warm thereafter. Inferred from whether\n"
        "                this campaign had already loaded the file, not observed:\n"
        "                purging the page cache needs privileges this does not take.\n"
        % sweep.get("page_cache_first_load", "unknown")
    )
    provenance = (records[0].get("provenance") if records else None) or {}
    out.write(
        "  engine        %s\n                %s\n"
        % (provenance.get("llama_server_version", "unknown"), provenance.get("llama_server_path", "unknown"))
    )
    out.write("\n")

    findings = harness.check_campaign(records, comparing=True)
    if findings:
        out.write("INVARIANTS\n")
        by_id = {}
        for record_id, invariant, detail in findings:
            entry = by_id.setdefault(invariant, {"detail": detail, "records": []})
            entry["records"].append(record_id)
        for invariant in sorted(by_id):
            entry = by_id[invariant]
            names = entry["records"]
            out.write("  %-6s %d record(s): %s\n" % (invariant, len(names), entry["detail"]))
            shown = ", ".join(names[:6]) + (", ..." if len(names) > 6 else "")
            out.write("         %s\n" % shown)
        out.write(
            "\n  This dataset is not usable as a configuration comparison. A fixed\n"
            "  run order makes thermal drift indistinguishable from a factor.\n\n"
        )
        return 1

    analysis, unmatched, mismatches = analyse(container)
    ungated = apply_quality_gate(analysis, container)
    curves = curve_analysis(container)
    if not analysis and not curves:
        out.write("No metric had both a baseline and a level. Nothing to compare.\n")
        return 1

    status = 0

    violations = sorted(
        {i for block in analysis.values() for i in block["baseline"]["disqualifiers"]}
    ) + sorted(
        {
            i
            for block in analysis.values()
            for row in block["rows"]
            for i in row["disqualifiers"]
        }
    )
    if violations:
        status = 1
        out.write("INVARIANT VIOLATIONS ON COMPARED RECORDS\n")
        for invariant in sorted(set(violations), key=invariant_order):
            out.write("  %s\n" % invariant)
        out.write(
            "\n  Any record carrying one of these is disqualified rather than\n"
            "  compared. I9 says so outright for speed work: a record with a\n"
            "  non-zero temperature or an unfixed seed is not admissible in a\n"
            "  speed comparison, whatever its numbers look like.\n\n"
        )

    provisional_metrics = [
        (key, block["provisional"]) for key, block in analysis.items() if block["provisional"]
    ]
    if provisional_metrics:
        out.write("PROVISIONAL UNDER I8\n")
        for key, reasons in provisional_metrics:
            out.write("  %-26s %s\n" % (key, "; ".join(reasons)))
        out.write(
            "\n  I8 puts the floor for any aggregate at five, and marks anything\n"
            "  below it provisional and excluded from comparisons. No verdict is\n"
            "  rendered against these figures. They are printed because a run that\n"
            "  happened should be readable, not because it decided anything.\n\n"
        )

    if analysis:
        out.write("BASELINE, DISPERSION AND DETECTION THRESHOLD\n")
        out.write(
            "  %-26s %10s %10s %11s %11s %11s\n"
            % ("metric", "median", "max", "within cv", "between cv", "threshold")
        )
    for key, block in analysis.items():
        base = block["baseline"]
        out.write(
            "  %-26s %10s %10s %11s %11s %10s %%   runs=%d, n=%d\n"
            % (
                key,
                fmt(base["median"]),
                fmt(base["max_of_run_medians"]),
                fmt(base["within_run_cv_median"], "%.4f"),
                fmt(base["between_run_cv"], "%.4f"),
                fmt(block["threshold_pct"], "%.1f"),
                base["n_runs"],
                base["n_per_run"],
            )
        )
    if analysis:
        out.write(
        "\n  `max` is the largest of the baseline run medians, not a tail estimate.\n"
        "  1.4.8: at n = 5 a p95 is the maximum, and a p95 carrying information\n"
        "  needs n >= 20. The column is named for what it is.\n"
        "\n  `between cv` is the dispersion of the baseline medians across runs,\n"
        "  not within one. A cv over consecutive requests on a live server holds\n"
        "  no restart and no thermal drift, and every level here required both.\n"
        "\n  `threshold` is the smallest relative difference this baseline can\n"
        "  resolve: t(0.975, runs-1) * cv * sqrt(1 + 1/runs), the half-width of a\n"
        "  two-sided 95 percent interval on the difference between the baseline\n"
        "  and one level run. It is roughly three times the cv at 5 runs. Reading\n"
        "  the bare cv as the bar would be a one-sigma test, which a third of null\n"
        "  comparisons clear by chance.\n"
        "\n  Detectable is not the same as important. A threshold near a tenth of a\n"
        "  percent means the baseline is highly reproducible, not that a one\n"
        "  percent difference matters. The verdict says only whether a difference\n"
        "  is real; the delta column says how big it is, and that is the column\n"
        "  that decides whether anything should be done about it.\n\n"
    )

    conflicts = detect_conflicts(analysis)
    if conflicts:
        out.write("FACTORS WHOSE TWO VERDICTS DISAGREE\n")
        for item in conflicts:
            out.write(
                "  %-14s %-14s cold %s %+.1f %%   warm %s %+.1f %%\n"
                % (
                    item["factor"],
                    "= " + str(item["level"]),
                    item["cold_dir"],
                    item["cold_delta"],
                    item["warm_dir"],
                    item["warm_delta"],
                )
            )
        out.write(
            "\n  Measured on %s for the cold axis and %s for the warm one.\n"
            "  These are not averaged and no single recommendation is derived from\n"
            "  them. A flag that buys cold prefill and sells warm continuation has\n"
            "  not been shown to be good or bad; it has been shown to be a trade,\n"
            "  and which side matters depends on the workload. An agentic loop is\n"
            "  almost entirely warm continuations.\n\n"
            % (HEADLINE[AXIS_COLD], HEADLINE[AXIS_WARM])
        )

    by_factor = {}
    for key, block in analysis.items():
        for row in block["rows"]:
            by_factor.setdefault(row["factor"], {}).setdefault(key, []).append(row)

    factor_meta = {
        f.get("name"): f for f in (sweep.get("factors") or []) if f.get("name")
    }
    interaction_meta = {
        i.get("name"): i for i in (sweep.get("interactions") or []) if i.get("name")
    }

    def role_of(name):
        rows = [r for metric in by_factor[name].values() for r in metric]
        return rows[0]["role"] if rows else "level"

    singles = sorted(n for n in by_factor if role_of(n) != "interaction")
    combined = sorted(n for n in by_factor if role_of(n) == "interaction")

    for name in singles:
        declared = factor_meta.get(name, {})
        out.write(
            "FACTOR %s   baseline level %s\n"
            % (name, declared.get("baseline_level", "?"))
        )
        if name in NOT_IN_RUST or any(f in NOT_IN_RUST for f in declared.get("fields") or []):
            out.write(
                "  No field in LlamaServerConfig. The runtime can reach this flag\n"
                "  through extra_args; acting on a result here would argue first\n"
                "  for adding the field.\n"
            )
        if declared.get("hold_slot_ctx"):
            out.write(
                "  n_ctx moves with this factor so each slot keeps its capacity:\n"
                "  two flags change together, and the held quantity is the slot.\n"
            )
        note = level_note(container, name)
        if note:
            out.write("  %s\n" % wrap_note(note))
        render_axes(analysis, by_factor[name], out)
        for item in conflicts:
            if item["factor"] == name:
                out.write(
                    "  DISAGREEMENT at %s: cold %s, warm %s. Not merged.\n"
                    % (item["level"], item["cold_dir"], item["warm_dir"])
                )
        out.write("\n")

    if combined:
        out.write("INTERACTIONS\n")
        out.write(
            "  Two flags moved together, deliberately and after each was varied\n"
            "  alone. These cells are not one-factor-at-a-time verdicts and are\n"
            "  reported separately so they cannot be read as one.\n\n"
        )
        for name in combined:
            declared = interaction_meta.get(name, {})
            out.write(
                "  %s   fields %s   baseline cell %s\n"
                % (
                    name,
                    ", ".join(declared.get("fields") or []),
                    declared.get("baseline_cell", "?"),
                )
            )
            swept = declared.get("fields_swept_individually") or []
            missing = [f for f in (declared.get("fields") or []) if f not in swept]
            if missing:
                out.write(
                    "  %s was not varied alone in this plan, so this cell has no\n"
                    "  single-factor verdict to be read against.\n"
                    % ", ".join(missing)
                )
            note = level_note(container, name)
            if note:
                out.write("  %s\n" % wrap_note(note))
            render_axes(analysis, by_factor[name], out)
            out.write("\n")

    if curves:
        out.write("PREFILL CURVE, PER LENGTH\n")
        out.write(
            "  Each length is its own comparison against its own baseline and its\n"
            "  own threshold. A single figure pooled across lengths would measure\n"
            "  which lengths fitted the slot, not how fast prefill ran.\n"
        )
        withheld_lengths = sorted(
            {
                row["prompt_tok"]
                for block in curves
                for row in block["rows"]
                if row["unstable_baseline"]
            }
        )
        if withheld_lengths:
            out.write(
                "  %s\n"
                % wrap_note(
                    "Lengths %s are withheld at every level: a baseline run "
                    "measured them with a within-run cv above the 0.10 of 1.4.8, "
                    "so the dispersion their threshold is derived from is not "
                    "usable. The runs are named on the rows rather than dropped, "
                    "and the deltas are still printed, so the size of what is "
                    "not being claimed stays visible."
                    % ", ".join(str(n) for n in withheld_lengths)
                )
            )
        for block in curves:
            out.write("  %s = %s\n" % (block["factor"], block["level"]))
            for row in block["rows"]:
                out.write(
                    "    %7d tok  baseline %8.1f  level %8.1f  %+6.1f %%   %s\n"
                    % (
                        row["prompt_tok"],
                        row["baseline_tps"],
                        row["level_tps"],
                        row["delta_pct"] or 0.0,
                        row["verdict"],
                    )
                )
            if block["missing_in_level"]:
                out.write(
                    "    lengths the baseline measured and this level did not, "
                    "excluded: %s\n"
                    % ", ".join(str(n) for n in block["missing_in_level"])
                )
            if block["absent_from_baseline"]:
                out.write(
                    "    lengths this level measured and the baseline did not, "
                    "excluded: %s\n"
                    % ", ".join(str(n) for n in block["absent_from_baseline"])
                )
        out.write("\n")

    if ungated:
        status = 1
        out.write("QUALITY GATE DECLARED BUT NOT APPLIED\n")
        for factor, level in ungated:
            out.write("  %s = %s\n" % (factor, level))
        out.write(
            "\n  The plan declared a quality gate for these levels and no score\n"
            "  comparison could be made, so nothing was checked. A gate that\n"
            "  passed and a gate that never ran must not read alike.\n\n"
        )

    if mismatches:
        status = 1
        out.write("REQUESTED CACHE STATE NOT ACHIEVED\n")
        for item in mismatches:
            out.write(
                "  %-46s %s = %s\n"
                % (
                    item["record_id"],
                    item.get("factor") or "baseline",
                    item.get("level") or "",
                )
            )
            out.write(
                "      %s requested, %d of %d sample(s) came back %s\n"
                % (
                    item["requested"],
                    item["off"],
                    item["of"],
                    "/".join(item["achieved"]) or "nothing",
                )
            )
        out.write(
            "\n  These records were selected by name, so they are still read as\n"
            "  what they are, and their gated metrics are withheld rather than\n"
            "  filed under the state they happened to achieve. On this hybrid\n"
            "  model an identical resend is served by a context checkpoint like\n"
            "  any other reuse, so a configuration that removes checkpoints makes\n"
            "  the resend cold. That is a result about the cache, and reporting\n"
            "  it inside a latency figure would put it where nobody would look.\n\n"
        )

    if unmatched:
        status = 1
        out.write("RECORDS THAT ENTERED NO COMPARISON\n")
        for item in unmatched:
            out.write(
                "  %-44s %s = %s\n"
                % (item["record_id"], item.get("factor") or "baseline", item.get("level") or "")
            )
            out.write("      %s\n" % item["reason"])
        out.write(
            "\n  These runs completed and are counted in the header. Without this\n"
            "  list a level dropped here would be indistinguishable from a level\n"
            "  the plan never declared.\n\n"
        )

    if excluded:
        status = 1
        out.write("FAILURES\n")
        for item in excluded:
            what = (
                "%s = %s" % (item["factor"], item.get("level"))
                if item.get("factor")
                else "baseline"
            )
            out.write("  run %02d  %s\n" % (item.get("run_index", -1), what))
            for line in str(item.get("reason", "")).splitlines()[:6]:
                out.write("          %s\n" % line)
        out.write("\n")

    defective = [r for r in records if (r.get("sweep") or {}).get("defective")]
    if defective:
        status = 1
        out.write("DEFECTIVE OUTPUT\n")
        for record in defective:
            out.write(
                "  %-40s %s\n"
                % (record["record_id"], ", ".join(record["sweep"]["defective"][:4]))
            )
        out.write(
            "\n  A faster configuration that produces worse text is not a result.\n\n"
        )
    return status


AXIS_ORDER = (AXIS_COLD, AXIS_WARM, AXIS_OTHER)


def render_axes(analysis, rows_by_metric, out):
    """One factor's metrics, grouped by verdict axis.

    Cold and warm are printed under their own headings and never combined into
    a single figure. A factor that helps one and hurts the other has two true
    verdicts, and a reader who is shown only their average has been told
    something that is true of neither.
    """
    for axis in AXIS_ORDER:
        keys = [
            key
            for key in analysis
            if analysis[key]["axis"] == axis and rows_by_metric.get(key)
        ]
        if not keys:
            continue
        out.write(
            "  %s\n    %-26s %10s %10s %8s %9s   %s\n"
            % (axis.upper(), "metric / level", "median", "max", "cv", "delta", "verdict")
        )
        for key in keys:
            out.write("    %s\n" % key)
            for row in sorted(rows_by_metric[key], key=lambda r: str(r["level"])):
                out.write(
                    "    %-26s %10s %10s %8s %8s %%   %s\n"
                    % (
                        "  " + str(row["level"]),
                        fmt(row["value"]),
                        fmt(row["p95"]),
                        fmt(row["within_cv"], "%.3f"),
                        fmt(row["delta_pct"], "%+.1f"),
                        row["verdict"],
                    )
                )


def detect_conflicts(analysis):
    """Levels whose cold and warm headline verdicts point opposite ways.

    The criterion this implements exists because `-ub` is a known instance: a
    wide micro-batch buys cold prefill and sells the warm continuation an
    agentic loop actually runs. Reporting a factor once, on whichever axis was
    measured, would have hidden that entirely.
    """
    cold = analysis.get(HEADLINE[AXIS_COLD])
    warm = analysis.get(HEADLINE[AXIS_WARM])
    if not cold or not warm:
        return []

    def directions(block):
        found = {}
        for row in block["rows"]:
            if not row["comparable"] or row["delta_pct"] is None:
                continue
            if row["verdict"].startswith(("better", "worse")):
                found[(row["factor"], row["level"])] = (
                    row["verdict"].split(",")[0],
                    row["delta_pct"],
                    row["role"],
                )
        return found

    cold_dirs = directions(cold)
    warm_dirs = directions(warm)
    conflicts = []
    for key in sorted(set(cold_dirs) & set(warm_dirs)):
        if cold_dirs[key][0] == warm_dirs[key][0]:
            continue
        conflicts.append(
            {
                "factor": key[0],
                "level": key[1],
                "role": cold_dirs[key][2],
                "cold_dir": cold_dirs[key][0],
                "cold_delta": cold_dirs[key][1],
                "warm_dir": warm_dirs[key][0],
                "warm_delta": warm_dirs[key][1],
            }
        )
    return conflicts


def level_note(container, factor):
    """The first note recorded on any record of this factor."""
    for record in container.get("records") or []:
        sweep = record.get("sweep") or {}
        if sweep.get("factor") == factor and sweep.get("note"):
            return sweep["note"]
    return None


def wrap_note(text, width=72, indent="  "):
    words = str(text).split()
    lines, current = [], ""
    for word in words:
        if current and len(current) + 1 + len(word) > width:
            lines.append(current)
            current = word
        else:
            current = (current + " " + word) if current else word
    if current:
        lines.append(current)
    return ("\n" + indent).join(lines)




# ---------------------------------------------------------------------------
# Ceiling check
# ---------------------------------------------------------------------------


def ceiling_check(paths, out):
    """Achieved effective bandwidth per model, against the shared bus.

    The programme's efficiency figures all divide a measured rate by a ceiling
    whose bandwidth term assumes contiguous reads. A mixture of experts model
    gathers its active parameters from experts scattered across the file, and a
    scattered read does not reach sequential bandwidth, so the ceiling is
    plausibly optimistic for exactly the model that appears to have headroom.

    This says which way the evidence falls, and states what it cannot settle.
    """
    machine = roofline.resolve_machine(roofline.detect_machine_id(), None, None)
    rows = []
    for path in paths:
        try:
            with open(path, "r", encoding="utf-8") as handle:
                container = json.load(handle)
        except (OSError, ValueError) as exc:
            out.write("  could not read %s: %s\n" % (path, exc))
            continue
        for record in container.get("records") or []:
            if record.get("probe") != "speed" or speed_variant(record) != "cold":
                continue
            if (record.get("sweep") or {}).get("role") not in (None, "baseline"):
                continue
            decode = dig(record, ("stats", "decode_tps", "median"))
            model_path = (record.get("provenance") or {}).get("model_path")
            n_ctx = (record.get("engine") or {}).get("n_ctx")
            if not (decode and model_path and n_ctx):
                continue
            try:
                model = roofline.describe_model(model_path)
            except (roofline.RooflineError, OSError):
                continue
            block = roofline.compute_roofline(
                model,
                machine,
                n_ctx,
                (record.get("engine") or {}).get("cache_type_k") or "f16",
                (record.get("engine") or {}).get("cache_type_v") or "f16",
                1,
            )
            rows.append(
                {
                    "label": record.get("label"),
                    "decode_tps": decode,
                    "bytes_per_token": block["bytes_per_token_read"],
                    "achieved_bps": decode * block["bytes_per_token_read"],
                    "peak_bps": block["bandwidth_bytes_per_s"],
                    "ceiling": block["decode_ceiling_tps"],
                    "is_moe": model["is_moe"],
                    "is_hybrid": model["is_hybrid"],
                    "expert_used": model.get("expert_used_count"),
                    "expert_count": model.get("expert_count"),
                    "params_active": model["params_active_per_token"],
                    "source": os.path.basename(path),
                    "record_id": record.get("record_id"),
                    "n_ctx": n_ctx,
                    "cache_type_k": (record.get("engine") or {}).get("cache_type_k") or "f16",
                    "cache_type_v": (record.get("engine") or {}).get("cache_type_v") or "f16",
                }
            )

    if not rows:
        out.write("No cold speed record with a readable model. Nothing to check.\n")
        return 1

    # One row per model. Which record won is decided by file order, so it is
    # printed rather than left to be reconstructed: a bandwidth figure whose
    # context length and source campaign are unstated cannot be checked.
    seen = {}
    duplicates = {}
    for row in rows:
        if row["label"] in seen:
            duplicates[row["label"]] = duplicates.get(row["label"], 0) + 1
            continue
        seen[row["label"]] = row
    rows = [seen[label] for label in sorted(seen)]

    out.write("ACHIEVED EFFECTIVE BANDWIDTH\n")
    out.write(
        "  %-20s %10s %14s %14s %10s  %s\n"
        % ("model", "decode", "bytes/token", "achieved", "of peak", "architecture")
    )
    for row in rows:
        arch = []
        if row["is_moe"]:
            arch.append("MoE %s/%s experts" % (row["expert_used"], row["expert_count"]))
        else:
            arch.append("dense")
        arch.append("hybrid attention" if row["is_hybrid"] else "full attention")
        out.write(
            "  %-20s %8.1f/s %11.2f GB %11.0f GB/s %9.1f %%  %s\n"
            % (
                row["label"],
                row["decode_tps"],
                row["bytes_per_token"] / 1e9,
                row["achieved_bps"] / 1e9,
                100.0 * row["achieved_bps"] / row["peak_bps"],
                ", ".join(arch),
            )
        )
        out.write(
            "  %-20s from %s, record %s, n_ctx %d, KV %s/%s%s\n"
            % (
                "",
                row["source"],
                row["record_id"],
                row["n_ctx"],
                row["cache_type_k"],
                row["cache_type_v"],
                (
                    ", %d further cold baseline record(s) for this model not used"
                    % duplicates[row["label"]]
                    if row["label"] in duplicates
                    else ""
                ),
            )
        )

    out.write("\nWHAT THIS DOES AND DOES NOT SETTLE\n")
    best = max(rows, key=lambda r: r["achieved_bps"])
    worst = min(rows, key=lambda r: r["achieved_bps"])
    if best is not worst:
        out.write(
            "  %s reaches %.0f GB/s on this bus and %s reaches %.0f GB/s, a factor\n"
            "  of %.1f. Both read from the same memory, so either the second model\n"
            "  is genuinely not bus-bound and has headroom, or its access pattern\n"
            "  cannot reach the rate a contiguous read achieves and the ceiling\n"
            "  formula in 1.4.11 needs a scatter term.\n"
            % (
                best["label"],
                best["achieved_bps"] / 1e9,
                worst["label"],
                worst["achieved_bps"] / 1e9,
                best["achieved_bps"] / max(worst["achieved_bps"], 1.0),
            )
        )
    moe = [r for r in rows if r["is_moe"]]
    hybrid = [r for r in rows if r["is_hybrid"]]
    if moe and hybrid and {r["label"] for r in moe} == {r["label"] for r in hybrid}:
        out.write(
            "\n  The two properties are perfectly confounded in this shortlist:\n"
            "  every mixture of experts model here is also a hybrid attention\n"
            "  model, and every dense one is also full attention. Nothing in this\n"
            "  data can attribute the deficit to expert gather rather than to the\n"
            "  recurrent path. A third model breaking the pair, dense-and-hybrid\n"
            "  or mixture-of-experts-with-full-attention, is what would settle it.\n"
        )
    render_ubatch_differential(paths, out)
    return 0


def ubatch_response(container):
    """Per-model prefill and decode response to the micro-batch width."""
    analysis, _unmatched, _mismatches = analyse(container)
    label = (container.get("sweep") or {}).get("model_label")
    if not label:
        return None
    out = {"label": label, "prefill": {}, "decode": {}}
    for metric, key in (("prefill_tps cold", "prefill"), ("decode_tps cold", "decode")):
        block = analysis.get(metric)
        if not block:
            continue
        out[key + "_threshold"] = block["threshold_pct"]
        for row in block["rows"]:
            if row["factor"] != "n_ubatch" or not row["comparable"]:
                continue
            # `comparable` means the row earned a verdict, which includes "no
            # detectable effect". Whether an effect was actually found is a
            # different question and the one the decode line asks.
            out[key][str(row["level"])] = {
                "delta_pct": row["delta_pct"],
                "detected": row["verdict"].startswith(("better", "worse")),
            }
    return out if out["prefill"] else None


def render_ubatch_differential(paths, out):
    """The discriminator that needs no third model.

    Batch width sets how many tokens share one gather of the active experts, so
    a cost that comes from gathering scattered weights is amortised by a wide
    micro-batch and a cost that comes from reading contiguous ones is not. The
    two architectures are compared on how much their prefill responds.
    """
    responses = []
    for path in paths:
        try:
            with open(path, "r", encoding="utf-8") as handle:
                container = json.load(handle)
        except (OSError, ValueError):
            continue
        if not (container.get("sweep") or {}).get("factors"):
            continue
        response = ubatch_response(container)
        if response:
            responses.append(response)

    out.write("\n  THE DISCRIMINATOR AVAILABLE WITHOUT A THIRD MODEL\n")
    if len(responses) < 2:
        out.write(
            "  Pass the sweep datasets for both models to compute it. Batch width\n"
            "  sets how many tokens share one gather of the active experts, so if\n"
            "  the deficit is a gather problem the mixture of experts model's\n"
            "  prefill responds to -ub materially more than the dense model's.\n"
        )
        return

    levels = sorted(
        {lvl for r in responses for lvl in r["prefill"]}, key=lambda v: int(v)
    )
    out.write("  prefill response to -ub, relative to the engine default\n")
    out.write("    %-22s %s\n" % ("model", "".join("%12s" % ("ub " + l) for l in levels)))
    for response in responses:
        out.write(
            "    %-22s %s\n"
            % (
                response["label"],
                "".join(
                    "%11s " % (
                        "%+.1f %%" % response["prefill"][l]["delta_pct"]
                        if l in response["prefill"]
                        else "-"
                    )
                    for l in levels
                ),
            )
        )
    out.write("  decode response to -ub, same runs\n")
    for response in responses:
        moved = [
            l for l in levels
            if l in response["decode"] and response["decode"][l]["detected"]
        ]
        out.write(
            "    %-22s %s\n"
            % (
                response["label"],
                "effect at " + ", ".join(moved)
                if moved
                else "no detectable effect at any level",
            )
        )

    shared = [
        l for l in levels if all(l in r["prefill"] for r in responses)
    ]
    ratios = []
    for level in shared:
        magnitudes = sorted(abs(r["prefill"][level]["delta_pct"]) for r in responses)
        if magnitudes[0] > 0:
            ratios.append(magnitudes[-1] / magnitudes[0])
    if ratios:
        out.write(
            "\n  The larger response exceeds the smaller by %.1f to %.1f times across\n"
            "  the levels both models ran.\n" % (min(ratios), max(ratios))
        )
    out.write(
        "\n  Decode is where this matters and where nothing can be done about it.\n"
        "  A decoded token is a batch of one by construction, so a cost that batch\n"
        "  width amortises during prefill cannot be amortised at decode at all.\n"
        "  A model carrying a large batch-amortisable cost in prefill therefore\n"
        "  pays it in full on every generated token, which is exactly the traffic\n"
        "  `decode_ceiling_tps` models as a contiguous sequential read.\n"
    )


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def parse_args(argv):
    parser = argparse.ArgumentParser(
        prog="sweep.py",
        description="Controlled parameter sweep against a frozen baseline.",
    )
    parser.add_argument("plan", nargs="?", help="path to the experiment plan, TOML")
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="print the full run plan and the estimated wall-clock, execute nothing",
    )
    parser.add_argument("--out", default=os.path.join(HERE, "results"), help="output directory")
    parser.add_argument(
        "--calibrate-from", help="dataset whose observed timings feed the estimate"
    )
    parser.add_argument("--report", help="render the report from an existing dataset")
    parser.add_argument(
        "--ceiling-check",
        nargs="*",
        default=None,
        metavar="CAMPAIGN",
        help="achieved bandwidth per model; defaults to results/baseline-*.json",
    )
    return parser.parse_args(argv)


def main(argv):
    args = parse_args(argv)
    out = sys.stdout

    if args.ceiling_check is not None:
        paths = args.ceiling_check or sorted(
            glob.glob(os.path.join(HERE, "results", "baseline-*.json"))
        )
        if not paths:
            sys.stderr.write("no campaign file to check\n")
            return 2
        return ceiling_check(paths, out)

    if args.report:
        try:
            with open(args.report, "r", encoding="utf-8") as handle:
                container = json.load(handle)
        except (OSError, ValueError) as exc:
            sys.stderr.write("cannot read %s: %s\n" % (args.report, exc))
            return 2
        return render_report(container, out)

    if not args.plan:
        sys.stderr.write("a plan file is required; see --help\n")
        return 2

    try:
        spec = load_plan(args.plan)
    except (PlanError, OSError, ValueError, tomllib.TOMLDecodeError) as exc:
        sys.stderr.write("plan: %s\n" % exc)
        return 2
    except roofline.RooflineError:
        sys.stderr.write(
            "plan: llama-server not found. Set [engine] binary in the plan, or "
            "APOLLIA_LLAMA_SERVER_BIN.\n"
        )
        return 2

    runs = build_runs(spec)
    os.makedirs(args.out, exist_ok=True)

    if args.dry_run:
        notes = preflight(spec, runs)
        estimation = estimate(spec, runs, args.out, args.calibrate_from)
        render_dry_run(spec, runs, estimation, notes, out)
        return 0

    scratch = os.path.join(args.out, "scratch")
    os.makedirs(scratch, exist_ok=True)
    try:
        dataset_path, container = execute(spec, runs, args.out, scratch)
    except PlanError as exc:
        sys.stderr.write("sweep: %s\n" % exc)
        return 2
    except KeyboardInterrupt:
        sys.stderr.write("\nsweep: interrupted, no dataset written\n")
        return 2

    out.write("\n[sweep] dataset %s\n\n" % dataset_path)
    status = render_report(container, out)
    return 1 if (status or container.get("records_excluded")) else 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
