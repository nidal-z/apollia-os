#!/usr/bin/env python3
"""Hold the artifact naming contract from the producer to every consumer.

The release pipeline had no contract of names: `apollia-os update` expected
ten `apollia-os-<triple>` binaries the workflow never compiled, the desktop
updater endpoint named a `latest.json` no step produced, and the docs cited
four installer names the bundler never writes. Each side was written alone,
and each was exact in its syntax and false in its content.

The contract is `packaging/artifacts.json`. This guard reads every side by
parsing, never by mention: a name cited in a comment counts for nothing, which
is precisely how the prototype instrument was fooled (a single comment line in
the workflow turned its manifest check green).

Rules:

  contract-form     the contract parses, presets and archives are unique,
                    each archive is `apollia-os-<preset>.tar.gz|zip`, and one
                    `self_update` archive exists per (os, arch) couple
  matrix-bijection  the presets of the release.yml matrix and of the contract
                    are the same set
  producer-names    the cli job reads its archive name from the contract (a
                    `jq` step over packaging/artifacts.json) and uploads that
                    name plus `.sha256`; with the bijection above, every
                    contract archive has a producer
  update-consumer   update.rs embeds the contract via include_str! and carries
                    no hardcoded release-asset name outside its tests
  endpoint-manifest the tauri.conf.json updater endpoint names the contract's
                    manifest under the contract's repo
  manifest-step     a step of the release job writes the manifests from the
                    contract, iterating `updater_manifests`; comments do not
                    count
  channel-endpoint  a desktop entry on a channel other than the default is
                    built by a job that points the updater there, so an
                    installed variant asks its own manifest for updates
  manifest-published every channel the contract declares is matched by an upload
                    pattern of the release job, so a composed manifest reaches
                    the release rather than staying in the runner
  signing-outputs   the artifact-signing step writes under a suffix that no
                    updater signature of the contract already occupies, and
                    that an upload pattern of the release job publishes
  updater-uploads   each desktop job stages the updater signature its contract
                    entry declares, in a run line that is not a comment
  flat-uploads      every upload-artifact path of the cli and desktop jobs
                    sits in one flat directory, because the release job reads
                    the merged artifacts with flat globs
  docs-block        the generated installer-name block of the install pages
                    matches the contract (regenerate with
                    `python3 scripts/check_release_artifacts.py --write-docs-block`,
                    which docs/site/regen.sh runs)

Exit codes, distinct on purpose so a run that measured nothing cannot pass for
a run that found nothing:

  0  every rule holds
  1  at least one defect, each printed with file and rule
  2  nothing was measured: a file to judge is absent, or PyYAML is missing

Usage:
    python3 scripts/check_release_artifacts.py
    python3 scripts/check_release_artifacts.py --selftest
    python3 scripts/check_release_artifacts.py --write-docs-block
"""

import argparse
import fnmatch
import json
import re
import shutil
import sys
import tempfile
from pathlib import Path

try:
    import yaml
except ImportError:  # pragma: no cover - the environment, not the tree
    print(
        "check_release_artifacts: PyYAML is absent, so nothing was measured",
        file=sys.stderr,
    )
    sys.exit(2)

REPO_ROOT = Path(__file__).resolve().parents[1]

CONTRACT = Path("packaging/artifacts.json")
WORKFLOW = Path(".github/workflows/release.yml")
UPDATE_RS = Path("crates/apollia-cli/src/commands/update.rs")
TAURI_CONF = Path("crates/apollia-desktop/tauri.conf.json")
DOCS_EN = Path("docs/site/docs/how-to/install-the-desktop-app.md")
DOCS_FR = Path(
    "docs/site/i18n/fr/docusaurus-plugin-content-docs/current/how-to/"
    "install-the-desktop-app.md"
)

FILES = (CONTRACT, WORKFLOW, UPDATE_RS, TAURI_CONF, DOCS_EN, DOCS_FR)

BLOCK_BEGIN = "<!-- release-artifacts:begin"
BLOCK_END = "<!-- release-artifacts:end -->"

# A hardcoded release-asset name in update.rs: the target-triple names of the
# old defect, or a full archive name. The contract is the only source.
HARDCODED_ASSET = re.compile(
    r'"apollia-os-[A-Za-z0-9_.-]*(?:musl|darwin|msvc|\.tar\.gz|\.zip)[A-Za-z0-9_.-]*"'
)

# The desktop job that builds a platform is named by the contract itself, in
# each entry's `job`. It used to be a table keyed on `os` here, which stopped
# working the day one system got two variants: Windows and Linux each ship a
# Vulkan bundle and a CUDA one, and an operating system no longer identifies a
# producer. Naming the job in the contract also puts the two sides of the
# question in one file rather than two.


def shell_lines(run: str) -> list[str]:
    """The effective lines of a `run:` block: stripped, comments dropped,
    backslash continuations joined so a wrapped command reads as one line."""
    joined: list[str] = []
    pending = ""
    for raw in run.splitlines():
        line = raw.strip()
        if line.endswith("\\"):
            pending += line[:-1] + " "
            continue
        joined.append(pending + line)
        pending = ""
    if pending:
        joined.append(pending)
    return [line for line in joined if line and not line.startswith("#")]


def matrix_presets(workflow: dict) -> list[str]:
    """The preset names of the embedded matrix heredoc of the setup job."""
    for step in workflow.get("jobs", {}).get("setup", {}).get("steps", []):
        run = step.get("run") or ""
        m = re.search(r"<<'JSON'.*?\n(.*?)\n\s*JSON\b", run, re.S)
        if m:
            body = m.group(1)
            # The heredoc is indented by the YAML block: dedent before parsing.
            data = json.loads("\n".join(line.strip() for line in body.splitlines()))
            return [e["preset"] for e in data.get("include", [])]
    return []


def upload_steps(job: dict) -> list[dict]:
    return [
        s
        for s in job.get("steps", [])
        if isinstance(s.get("uses"), str) and s["uses"].startswith("actions/upload-artifact@")
    ]


def render_block(contract: dict, version: str, lang: str) -> str:
    """The generated installer-name block of the install pages."""
    if lang == "fr":
        head = "| Plateforme | Fichiers sur la page de release |"
        labels = {
            "darwin-aarch64": "macOS (Apple Silicon)",
            "linux-x86_64": "Linux (x86-64)",
            "windows-x86_64": "Windows (x86-64)",
            "linux-x86_64-cuda": "Linux (x86-64), moteur CUDA",
            "windows-x86_64-cuda": "Windows (x86-64), moteur CUDA",
        }
        origin = (
            f"{BLOCK_BEGIN} - genere depuis packaging/artifacts.json par "
            "docs/site/regen.sh ; ne pas editer a la main -->"
        )
    else:
        head = "| Platform | Files on the release page |"
        labels = {
            "darwin-aarch64": "macOS (Apple Silicon)",
            "linux-x86_64": "Linux (x86-64)",
            "windows-x86_64": "Windows (x86-64)",
            "linux-x86_64-cuda": "Linux (x86-64), CUDA engine",
            "windows-x86_64-cuda": "Windows (x86-64), CUDA engine",
        }
        origin = (
            f"{BLOCK_BEGIN} - generated from packaging/artifacts.json by "
            "docs/site/regen.sh; do not edit by hand -->"
        )
    rows = [origin, head, "|---|---|"]
    for entry in contract["desktop"]:
        names = ", ".join(
            "`" + n.replace("{version}", version) + "`" for n in entry["installers"]
        )
        rows.append(f"| {labels.get(entry['platform'], entry['platform'])} | {names} |")
    rows.append(BLOCK_END)
    return "\n".join(rows)


def splice_block(text: str, block: str) -> str | None:
    """Replace the marker block inside a page, or None when markers are absent."""
    begin = text.find(BLOCK_BEGIN)
    end = text.find(BLOCK_END)
    if begin == -1 or end == -1 or end < begin:
        return None
    return text[:begin] + block + text[end + len(BLOCK_END) :]


def current_block(text: str) -> str | None:
    begin = text.find(BLOCK_BEGIN)
    end = text.find(BLOCK_END)
    if begin == -1 or end == -1 or end < begin:
        return None
    return text[begin : end + len(BLOCK_END)]


def check(root: Path) -> list[str] | int:
    """Every defect found under `root`, or 2 when nothing could be measured."""
    for rel in FILES:
        if not (root / rel).is_file():
            print(f"check_release_artifacts: {rel} is absent, nothing was measured")
            return 2

    defects: list[str] = []

    # ── contract-form ─────────────────────────────────────────────────────
    try:
        contract = json.loads((root / CONTRACT).read_text(encoding="utf-8"))
    except json.JSONDecodeError as err:
        return [f"{CONTRACT}: contract-form: not valid JSON ({err})"]

    cli = contract.get("cli", [])
    presets = [e.get("preset") for e in cli]
    archives = [e.get("archive") for e in cli]
    if len(set(presets)) != len(presets):
        defects.append(f"{CONTRACT}: contract-form: duplicated preset")
    if len(set(archives)) != len(archives):
        defects.append(f"{CONTRACT}: contract-form: duplicated archive name")
    for entry in cli:
        expected = re.compile(rf"^apollia-os-{re.escape(entry['preset'])}\.(tar\.gz|zip)$")
        if not expected.match(entry.get("archive", "")):
            defects.append(
                f"{CONTRACT}: contract-form: archive {entry.get('archive')!r} does not "
                f"follow apollia-os-{entry['preset']}.tar.gz|zip"
            )
    couples = [(e["os"], e["arch"]) for e in cli if e.get("self_update")]
    if len(set(couples)) != len(couples):
        defects.append(f"{CONTRACT}: contract-form: duplicated self_update (os, arch) couple")

    # ── the workflow, parsed once ─────────────────────────────────────────
    workflow = yaml.safe_load((root / WORKFLOW).read_text(encoding="utf-8"))
    jobs = workflow.get("jobs", {})

    # ── matrix-bijection ──────────────────────────────────────────────────
    wf_presets = matrix_presets(workflow)
    if not wf_presets:
        defects.append(f"{WORKFLOW}: matrix-bijection: no embedded matrix found")
    else:
        missing = sorted(set(presets) - set(wf_presets))
        extra = sorted(set(wf_presets) - set(presets))
        if missing:
            defects.append(
                f"{WORKFLOW}: matrix-bijection: contract preset(s) with no producer: {missing}"
            )
        if extra:
            defects.append(
                f"{WORKFLOW}: matrix-bijection: matrix preset(s) absent from the contract: {extra}"
            )

    # ── producer-names ────────────────────────────────────────────────────
    cli_job = jobs.get("cli", {})
    contract_step_ok = any(
        "jq" in line and "packaging/artifacts.json" in line and ".archive" in line
        for step in cli_job.get("steps", [])
        if isinstance(step.get("run"), str)
        for line in shell_lines(step["run"])
    )
    if not contract_step_ok:
        defects.append(
            f"{WORKFLOW}: producer-names: the cli job has no step reading the archive "
            f"name from {CONTRACT} with jq"
        )
    cli_uploads = upload_steps(cli_job)
    wants = "${{ steps.contract.outputs.archive }}"
    upload_paths = [
        p.strip()
        for step in cli_uploads
        for p in str(step.get("with", {}).get("path", "")).splitlines()
        if p.strip()
    ]
    if not any(p.endswith(wants) for p in upload_paths):
        defects.append(
            f"{WORKFLOW}: producer-names: no cli upload path publishes the contract archive name"
        )
    if not any(p.endswith(wants + ".sha256") for p in upload_paths):
        defects.append(
            f"{WORKFLOW}: producer-names: no cli upload path publishes the .sha256 companion"
        )

    # ── update-consumer ───────────────────────────────────────────────────
    update_src = (root / UPDATE_RS).read_text(encoding="utf-8")
    include = re.search(r'include_str!\("([^"]+)"\)', update_src)
    if not include:
        defects.append(f"{UPDATE_RS}: update-consumer: no include_str! of the contract")
    else:
        resolved = ((root / UPDATE_RS).parent / include.group(1)).resolve()
        if resolved != (root / CONTRACT).resolve():
            defects.append(
                f"{UPDATE_RS}: update-consumer: include_str! resolves to {resolved}, "
                f"not to {CONTRACT}"
            )
    production = update_src.split("#[cfg(test)]", 1)[0]
    for literal in HARDCODED_ASSET.findall(production):
        defects.append(
            f"{UPDATE_RS}: update-consumer: hardcoded release-asset name {literal} "
            f"outside the tests; the contract is the only source"
        )

    # ── endpoint-manifest ─────────────────────────────────────────────────
    conf = json.loads((root / TAURI_CONF).read_text(encoding="utf-8"))
    endpoints = conf.get("plugins", {}).get("updater", {}).get("endpoints", [])
    manifest = contract.get("updater_manifest", "latest.json")
    if not endpoints:
        defects.append(f"{TAURI_CONF}: endpoint-manifest: no updater endpoint")
    else:
        endpoint = endpoints[0]
        if endpoint.rsplit("/", 1)[-1] != manifest:
            defects.append(
                f"{TAURI_CONF}: endpoint-manifest: endpoint serves "
                f"{endpoint.rsplit('/', 1)[-1]!r}, the contract names {manifest!r}"
            )
        if contract.get("repo", "") not in endpoint:
            defects.append(
                f"{TAURI_CONF}: endpoint-manifest: endpoint does not sit under "
                f"the contract repo {contract.get('repo')!r}"
            )

    # ── manifest-step ─────────────────────────────────────────────────────
    # The manifest name itself lives in the contract, so the step is judged on
    # reading the contract and its `updater_manifest` key in effective lines,
    # never on a name cited in a comment.
    release_job = jobs.get("release", {})
    manifest_step_ok = False
    for step in release_job.get("steps", []):
        if not isinstance(step.get("run"), str):
            continue
        lines = shell_lines(step["run"])
        if any(str(CONTRACT) in line for line in lines) and any(
            "updater_manifests" in line for line in lines
        ):
            manifest_step_ok = True
            break
    if not manifest_step_ok:
        defects.append(
            f"{WORKFLOW}: manifest-step: no release step composes {manifest} from "
            f"the contract's updater_manifest (comments do not count)"
        )

    # ── channel-endpoint ──────────────────────────────────────────────────
    # A Tauri manifest keys platforms by triple, so two bundles sharing a
    # triple cannot share a manifest: the second written wins and every
    # installed copy is then offered whichever engine was composed last. A
    # variant on its own channel must therefore be built pointing at that
    # channel, and the endpoint is baked in at build time.
    default_manifest = contract.get("updater_manifest", "latest.json")
    for entry in contract.get("desktop", []):
        channel = entry.get("manifest", default_manifest)
        if channel == default_manifest:
            continue
        job_id = entry.get("job")
        job = jobs.get(job_id, {})
        points_there = any(
            channel in line
            for step in job.get("steps", [])
            if isinstance(step.get("run"), str)
            for line in shell_lines(step["run"])
        )
        if not points_there:
            defects.append(
                f"{WORKFLOW}: channel-endpoint: {entry['platform']} declares the "
                f"channel {channel!r} and job {job_id} never points the updater "
                f"at it, so the bundle would ask {default_manifest!r} and be "
                f"offered another variant"
            )

    # ── updater-uploads ───────────────────────────────────────────────────
    for entry in contract.get("desktop", []):
        job_id = entry.get("job")
        if job_id not in jobs:
            defects.append(
                f"{CONTRACT}: updater-uploads: platform {entry['platform']} names "
                f"job {job_id!r}, which {WORKFLOW} does not define"
            )
            continue
        job = jobs[job_id]
        # `updater: null` is a bundle that does not self-update, and says so
        # rather than leaving the reader to infer it from a missing key. The
        # CUDA variants are that case: Tauri resolves one manifest per platform
        # triple, so two bundles for one triple cannot both be offered by it.
        if entry.get("updater") is None:
            continue
        sig = entry["updater"]["signature"]
        # The extension suffix, stable across the {version} placeholder,
        # e.g. ".AppImage.sig", ".exe.sig", ".gz.sig".
        sig_suffix = "." + ".".join(sig.split(".")[-2:])
        staged = any(
            sig_suffix in line
            for step in job.get("steps", [])
            if isinstance(step.get("run"), str)
            for line in shell_lines(step["run"])
        )
        if not staged:
            defects.append(
                f"{WORKFLOW}: updater-uploads: job {job_id} stages no updater "
                f"signature (*{sig_suffix}) in any run line"
            )

    # ── flat-uploads ──────────────────────────────────────────────────────
    desktop_jobs = [e["job"] for e in contract.get("desktop", []) if e.get("job")]
    for job_id in ["cli", *dict.fromkeys(desktop_jobs)]:
        for step in upload_steps(jobs.get(job_id, {})):
            paths = [
                p.strip()
                for p in str(step.get("with", {}).get("path", "")).splitlines()
                if p.strip()
            ]
            parents = {str(Path(p).parent) for p in paths}
            if len(parents) > 1:
                defects.append(
                    f"{WORKFLOW}: flat-uploads: job {job_id} uploads from several "
                    f"directories {sorted(parents)}; the release job reads the merged "
                    f"artifacts with flat globs"
                )

    # ── manifest-published ────────────────────────────────────────────────
    # Composing a channel and publishing it are two different acts, and only
    # the first was checked. `latest-cuda.json` was written by the release job
    # and matched by none of its upload globs, so a CUDA bundle would have
    # asked a file that never reached the release and been told nothing is
    # available, for as long as nobody looked.
    release_files: list[str] = []
    for step in jobs.get("release", {}).get("steps", []):
        with_ = step.get("with") or {}
        if "draft" in with_:
            release_files = [
                line.strip().removeprefix("artifacts/")
                for line in str(with_.get("files", "")).splitlines()
                if line.strip()
            ]
    for manifest in contract.get("updater_manifests", []):
        if not any(fnmatch.fnmatch(manifest, pattern) for pattern in release_files):
            defects.append(
                f"{WORKFLOW}: manifest-published: the contract declares the channel "
                f"{manifest!r} and no upload pattern of the release job matches it, "
                f"so the file is composed and then left behind"
            )

    # ── signing-outputs ───────────────────────────────────────────────────
    # The signing step derives its output name from the file it signs. When
    # that derivation lands on a name the contract already owns, signing stops
    # being additive: `--output-signature "$f.sig"` overwrote the four updater
    # signatures produced by the desktop jobs, after latest.json had read them
    # and after SHA256SUMS had hashed them, so the release published an
    # inventory that no longer matched its own files.
    updater_suffixes = {}
    for entry in contract.get("desktop", []):
        updater = entry.get("updater")
        if not updater:
            continue
        artifact, signature = updater["artifact"], updater["signature"]
        if signature.startswith(artifact):
            updater_suffixes[signature[len(artifact) :]] = signature

    sign_run = ""
    for step in jobs.get("release", {}).get("steps", []):
        if "sign artifacts" in str(step.get("name", "")).lower():
            sign_run = str(step.get("run", ""))
    if not sign_run:
        defects.append(
            f"{WORKFLOW}: signing-outputs: no step of the release job is named "
            f"'Sign artifacts', so nothing signs what the release publishes"
        )
    outputs = r'--(?:bundle|output-signature|output-certificate) "\$f([^"]*)"'
    written = set(re.findall(outputs, sign_run))
    if sign_run and not written:
        defects.append(
            f"{WORKFLOW}: signing-outputs: the signing step names no output derived "
            f"from the file it signs, so what it writes cannot be checked"
        )
    for suffix in sorted(written):
        if suffix in updater_suffixes:
            defects.append(
                f"{WORKFLOW}: signing-outputs: the signing step writes '$f{suffix}', which is "
                f"the suffix of the updater signature {updater_suffixes[suffix]!r} the contract "
                f"declares, so signing overwrites a file the release already produced"
            )
        if not any(fnmatch.fnmatch(f"any{suffix}", pattern) for pattern in release_files):
            defects.append(
                f"{WORKFLOW}: signing-outputs: the signing step writes '$f{suffix}' and no "
                f"upload pattern of the release job matches it, so the signature is written "
                f"and then left behind"
            )

    # ── docs-block ────────────────────────────────────────────────────────
    version = conf.get("version", "")
    for page, lang in ((DOCS_EN, "en"), (DOCS_FR, "fr")):
        text = (root / page).read_text(encoding="utf-8")
        found = current_block(text)
        wanted = render_block(contract, version, lang)
        if found is None:
            defects.append(f"{page}: docs-block: the generated block markers are absent")
        elif found != wanted:
            defects.append(
                f"{page}: docs-block: the block drifted from the contract; regenerate "
                f"with `python3 scripts/check_release_artifacts.py --write-docs-block`"
            )

    return defects


def write_docs_block(root: Path) -> int:
    """Regenerate the installer-name block of both install pages."""
    contract = json.loads((root / CONTRACT).read_text(encoding="utf-8"))
    version = json.loads((root / TAURI_CONF).read_text(encoding="utf-8"))["version"]
    for page, lang in ((DOCS_EN, "en"), (DOCS_FR, "fr")):
        text = (root / page).read_text(encoding="utf-8")
        spliced = splice_block(text, render_block(contract, version, lang))
        if spliced is None:
            print(f"{page}: no block markers to splice into", file=sys.stderr)
            return 1
        (root / page).write_text(spliced, encoding="utf-8")
        print(f"wrote the generated block into {page}")
    return 0


# ── selftest ──────────────────────────────────────────────────────────────


def _copy_tree(dest: Path) -> None:
    for rel in FILES:
        target = dest / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(REPO_ROOT / rel, target)


def selftest() -> int:
    """Prove each rule can fire, then that the pristine tree is green."""
    failures: list[str] = []

    def case(name: str, mutate, expect_rule: str) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            _copy_tree(root)
            mutate(root)
            result = check(root)
            fired = isinstance(result, list) and any(expect_rule in d for d in result)
            status = "ok  " if fired else "FAIL"
            print(f"  {status} {name}")
            if not fired:
                failures.append(name)

    def drop_manifest_step(root: Path) -> None:
        text = (root / WORKFLOW).read_text(encoding="utf-8")
        (root / WORKFLOW).write_text(
            text.replace("updater_manifest", "absent_key"), encoding="utf-8"
        )

    def comment_only_manifest(root: Path) -> None:
        # The exact trap that fooled the prototype: the manifest composition
        # exists only inside comments. Rewrite every effective line touching
        # it into a comment mentioning it.
        text = (root / WORKFLOW).read_text(encoding="utf-8")
        out = []
        for line in text.splitlines():
            stripped = line.strip()
            if "updater_manifest" in stripped and not stripped.startswith("#"):
                indent = line[: len(line) - len(line.lstrip())]
                out.append(f"{indent}# {stripped}")
            else:
                out.append(line)
        (root / WORKFLOW).write_text("\n".join(out) + "\n", encoding="utf-8")

    def drop_a_preset(root: Path) -> None:
        text = (root / WORKFLOW).read_text(encoding="utf-8")
        out = [line for line in text.splitlines() if '"preset":"linux-arm-cpu"' not in line]
        (root / WORKFLOW).write_text("\n".join(out) + "\n", encoding="utf-8")

    def hardcode_a_name(root: Path) -> None:
        text = (root / UPDATE_RS).read_text(encoding="utf-8")
        needle = "// ─── Core logic"
        text = text.replace(
            needle,
            'const OLD: &str = "apollia-os-x86_64-unknown-linux-musl";\n' + needle,
            1,
        )
        (root / UPDATE_RS).write_text(text, encoding="utf-8")

    def nest_an_upload(root: Path) -> None:
        text = (root / WORKFLOW).read_text(encoding="utf-8")
        text = text.replace(
            "path: target/x86_64-unknown-linux-gnu/release/bundle/dist/*",
            "path: |\n"
            "            target/x86_64-unknown-linux-gnu/release/bundle/dist/*.deb\n"
            "            target/x86_64-unknown-linux-gnu/release/bundle/appimage/*.AppImage",
            1,
        )
        (root / WORKFLOW).write_text(text, encoding="utf-8")

    def drift_docs_block(root: Path) -> None:
        text = (root / DOCS_EN).read_text(encoding="utf-8")
        (root / DOCS_EN).write_text(
            text.replace(".dmg`", ".dmg-renamed`"), encoding="utf-8"
        )

    print("selftest: each rule fires on a mutated copy")
    case("a manifest without a step is reported", drop_manifest_step, "manifest-step")
    case("a comment naming the manifest does not count", comment_only_manifest, "manifest-step")
    case("a contract preset without a producer is reported", drop_a_preset, "matrix-bijection")
    case("a hardcoded asset name in update.rs is reported", hardcode_a_name, "update-consumer")
    case("a nested upload path is reported", nest_an_upload, "flat-uploads")
    case("a drifted docs block is reported", drift_docs_block, "docs-block")

    # Positive control: the pristine tree is green, so the cases above fired
    # because of their mutation, not because the check reds everything.
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        _copy_tree(root)
        result = check(root)
        clean = isinstance(result, list) and not result
        print(f"  {'ok  ' if clean else 'FAIL'} positive control: the pristine tree is green")
        if not clean:
            failures.append(f"pristine tree not green: {result}")

    if failures:
        print(f"\nselftest: {len(failures)} case(s) failed", file=sys.stderr)
        return 1
    print("\nselftest: every case holds")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(
        description="artifact naming contract: producer and consumers agree"
    )
    parser.add_argument("--selftest", action="store_true", help="prove each rule can fire")
    parser.add_argument(
        "--write-docs-block",
        action="store_true",
        help="regenerate the installer-name block of the install pages",
    )
    args = parser.parse_args()

    if args.selftest:
        return selftest()
    if args.write_docs_block:
        return write_docs_block(REPO_ROOT)

    result = check(REPO_ROOT)
    if isinstance(result, int):
        return result
    if result:
        print(f"check_release_artifacts: {len(result)} defect(s)")
        for defect in result:
            print(f"  {defect}")
        return 1
    print("check_release_artifacts: the contract binds producer and consumers")
    return 0


if __name__ == "__main__":
    sys.exit(main())
