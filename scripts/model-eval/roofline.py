#!/usr/bin/env python3
"""Roofline calculator: theoretical ceilings for a GGUF model on a given machine.

Answers the only question that makes an optimisation target defensible: is a
measured throughput bad, or is it already close to what the hardware allows.

Reads the GGUF header directly (no dependency, no model load), derives total and
active parameters, effective bits per weight, KV cache size, then computes the
memory-bandwidth-bound decode ceiling and the compute-bound prefill ceiling.
Given a measurement record it prints measured over ceiling as a percentage.

Field names and record shape follow the measurement contract in README.md,
sections 1.3.10 and 1.5. No field name is invented here.

Usage:
  roofline.py --gguf <path> [--ctx N] [--machine ID] [--json]
  roofline.py --matrix [--measured <campaign.json>] [--models-dir DIR]
  roofline.py --validate-kv --gguf <path> --ctx 4096,16384

Stdlib only. Uses argparse rather than the environment-variable convention of
the sibling probes because this script has several orthogonal modes; argparse is
standard library, so the no-new-dependency invariant holds.
"""

import argparse
import glob
import json
import math
import os
import re
import struct
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
RUN_MATRIX = os.path.join(HERE, "run-matrix.sh")

MIB = 1024.0 * 1024.0
GIB = 1024.0 * 1024.0 * 1024.0

# ---------------------------------------------------------------------------
# ggml type traits
# ---------------------------------------------------------------------------

# (block size in elements, block size in bytes) per ggml type id.
# Transcribed from llama.cpp: enum ggml_type in ggml/include/ggml.h, and the
# type_traits table plus the block struct static_asserts in
# ggml/src/ggml-common.h. ggml_half is 2 bytes, K_SCALE_SIZE is 12, QK_K is 256.
# Sizes are exact, so tensor bytes are computed rather than inferred from
# general.file_type, which only names the dominant quantisation.
GGML_TYPES = {
    0: ("F32", 1, 4),
    1: ("F16", 1, 2),
    2: ("Q4_0", 32, 18),  # half + 32/2
    3: ("Q4_1", 32, 20),  # 2*half + 32/2
    6: ("Q5_0", 32, 22),  # half + u32 + 32/2
    7: ("Q5_1", 32, 24),  # 2*half + u32 + 32/2
    8: ("Q8_0", 32, 34),  # half + 32
    9: ("Q8_1", 32, 36),  # 2*half + 32
    10: ("Q2_K", 256, 84),  # 2*half + 256/16 + 256/4
    11: ("Q3_K", 256, 110),  # half + 256/4 + 256/8 + 12
    12: ("Q4_K", 256, 144),  # 2*half + 12 + 256/2
    13: ("Q5_K", 256, 176),  # 2*half + 12 + 256/2 + 256/8
    14: ("Q6_K", 256, 210),  # half + 256/16 + 3*256/4
    15: ("Q8_K", 256, 292),  # f32 + 256 + 256/16*i16
    16: ("IQ2_XXS", 256, 66),
    17: ("IQ2_XS", 256, 74),
    18: ("IQ3_XXS", 256, 98),
    19: ("IQ1_S", 256, 50),
    20: ("IQ4_NL", 32, 18),
    21: ("IQ3_S", 256, 110),
    22: ("IQ2_S", 256, 82),
    23: ("IQ4_XS", 256, 136),
    24: ("I8", 1, 1),
    25: ("I16", 1, 2),
    26: ("I32", 1, 4),
    27: ("I64", 1, 8),
    28: ("F64", 1, 8),
    29: ("IQ1_M", 256, 56),
    30: ("BF16", 1, 2),
    34: ("TQ1_0", 256, 54),
    35: ("TQ2_0", 256, 66),
    39: ("MXFP4", 32, 17),  # u8 scale + 32/2
    40: ("NVFP4", 64, 36),  # 4 u8 scales + 64/2
    41: ("Q1_0", 128, 18),  # half + 128/8
}

# Cache types accepted by llama-server -ctk / -ctv, as bytes per element.
CACHE_TYPES = {
    "f32": 4.0,
    "f16": 2.0,
    "bf16": 2.0,
    "q8_0": 34.0 / 32.0,
    "q5_1": 24.0 / 32.0,
    "q5_0": 22.0 / 32.0,
    "q4_1": 20.0 / 32.0,
    "q4_0": 18.0 / 32.0,
}

# ---------------------------------------------------------------------------
# Machine table
# ---------------------------------------------------------------------------


def apple_gpu_peak_flops(cores, clock_ghz, half_precision=True):
    """Peak FLOP/s for an Apple GPU, derived rather than quoted.

    Apple publishes no FLOPS figure for its GPUs. Each core holds 128 FP32 ALUs
    and one fused multiply-add counts as two operations, so the FP32 peak is
    cores * 128 * 2 * clock. Metal executes FP16 at twice the FP32 rate. The
    result is an upper bound that no real kernel reaches.
    """
    fp32 = cores * 128 * 2 * clock_ghz * 1e9
    return fp32 * 2.0 if half_precision else fp32


# Keyed by the identifier reported by `sysctl -n hw.model`, so a record can be
# matched back to the machine that produced it. Every figure carries its source.
# Adding a machine means adding an entry here, never editing a figure in place.
MACHINES = {
    "Mac15,14": {
        "chip": "Apple M3 Ultra",
        "gpu_cores": 60,
        "bandwidth_bytes_per_s": 819.0e9,
        "peak_flops": apple_gpu_peak_flops(60, 1.38),
        "sources": {
            "bandwidth_bytes_per_s": (
                "Apple M3 Ultra product specification, 819 GB/s unified memory "
                "bandwidth. Theoretical peak, not achievable bandwidth."
            ),
            "peak_flops": (
                "Derived, no vendor figure exists: 60 cores * 128 FP32 ALUs * 2 "
                "FLOP per FMA * 1.38 GHz = 21.2 TFLOP/s FP32, doubled for FP16 "
                "= 42.4 TFLOP/s. Upper bound."
            ),
        },
    },
    "Mac15,11": {
        "chip": "Apple M3 Max (40-core GPU)",
        "gpu_cores": 40,
        "bandwidth_bytes_per_s": 400.0e9,
        "peak_flops": apple_gpu_peak_flops(40, 1.38),
        "sources": {
            "bandwidth_bytes_per_s": (
                "Apple M3 Max product specification, 400 GB/s unified memory "
                "bandwidth."
            ),
            "peak_flops": (
                "Derived: 40 cores * 128 FP32 ALUs * 2 FLOP per FMA * 1.38 GHz, "
                "doubled for FP16."
            ),
        },
    },
    "generic-cuda": {
        "chip": "unspecified CUDA device",
        "gpu_cores": None,
        "bandwidth_bytes_per_s": None,
        "peak_flops": None,
        "sources": {
            "bandwidth_bytes_per_s": (
                "Not set. Pass --bandwidth with the vendor figure for the card "
                "under test, and record where it came from."
            ),
            "peak_flops": "Not set. Pass --peak-flops.",
        },
    },
}


def detect_machine_id():
    """Hardware identifier of the running machine, or None off macOS."""
    out = run_capture(["sysctl", "-n", "hw.model"])
    return out.strip() if out else None


def resolve_machine(machine_id, bandwidth_override, flops_override):
    """Machine entry plus the overrides applied, or an explicit failure."""
    if machine_id is None:
        machine_id = detect_machine_id()
    if machine_id is None:
        raise RooflineError(
            "cannot detect the machine identifier. Pass --machine with one of: "
            + ", ".join(sorted(MACHINES))
        )
    if machine_id not in MACHINES:
        raise RooflineError(
            "unknown machine '%s'. Known identifiers: %s. Add an entry to "
            "MACHINES with its sources, or pass --bandwidth and --peak-flops."
            % (machine_id, ", ".join(sorted(MACHINES)))
        )
    entry = dict(MACHINES[machine_id])
    entry["machine_id"] = machine_id
    entry["sources"] = dict(entry["sources"])
    if bandwidth_override is not None:
        entry["bandwidth_bytes_per_s"] = bandwidth_override
        entry["sources"]["bandwidth_bytes_per_s"] = "--bandwidth on the command line"
    if flops_override is not None:
        entry["peak_flops"] = flops_override
        entry["sources"]["peak_flops"] = "--peak-flops on the command line"
    if entry["bandwidth_bytes_per_s"] is None:
        raise RooflineError(
            "machine '%s' has no memory bandwidth figure. Pass --bandwidth."
            % machine_id
        )
    return entry


class RooflineError(Exception):
    """Anything that makes a ceiling undefined. Never a silent default."""


# ---------------------------------------------------------------------------
# GGUF header parsing
# ---------------------------------------------------------------------------

GGUF_MAGIC = 0x46554747  # "GGUF" little-endian

# GGUF metadata value types.
_T_UINT8, _T_INT8, _T_UINT16, _T_INT16 = 0, 1, 2, 3
_T_UINT32, _T_INT32, _T_FLOAT32, _T_BOOL = 4, 5, 6, 7
_T_STRING, _T_ARRAY, _T_UINT64, _T_INT64, _T_FLOAT64 = 8, 9, 10, 11, 12

_SCALAR_FORMATS = {
    _T_UINT8: ("<B", 1),
    _T_INT8: ("<b", 1),
    _T_UINT16: ("<H", 2),
    _T_INT16: ("<h", 2),
    _T_UINT32: ("<I", 4),
    _T_INT32: ("<i", 4),
    _T_FLOAT32: ("<f", 4),
    _T_BOOL: ("<B", 1),
    _T_UINT64: ("<Q", 8),
    _T_INT64: ("<q", 8),
    _T_FLOAT64: ("<d", 8),
}

SHARD_RE = re.compile(r"^(?P<stem>.+)-(?P<index>\d{5})-of-(?P<total>\d{5})\.gguf$")
BLOCK_RE = re.compile(r"^blk\.(?P<index>\d+)\.(?P<suffix>.+)$")


class _Reader:
    """Sequential reader over a file handle, little-endian."""

    def __init__(self, handle):
        self.handle = handle

    def take(self, count):
        blob = self.handle.read(count)
        if len(blob) != count:
            raise RooflineError("truncated GGUF header")
        return blob

    def scalar(self, value_type):
        fmt, size = _SCALAR_FORMATS[value_type]
        value = struct.unpack(fmt, self.take(size))[0]
        return bool(value) if value_type == _T_BOOL else value

    def string(self):
        length = struct.unpack("<Q", self.take(8))[0]
        return self.take(length).decode("utf-8", errors="replace")

    def value(self, value_type):
        if value_type == _T_STRING:
            return self.string()
        if value_type == _T_ARRAY:
            elem_type = struct.unpack("<I", self.take(4))[0]
            count = struct.unpack("<Q", self.take(8))[0]
            return [self.value(elem_type) for _ in range(count)]
        if value_type not in _SCALAR_FORMATS:
            raise RooflineError("unknown GGUF value type %d" % value_type)
        return self.scalar(value_type)


def tensor_bytes(n_elements, ggml_type):
    """Exact storage size of a tensor, from the ggml block layout."""
    if ggml_type not in GGML_TYPES:
        raise RooflineError(
            "unknown ggml type id %d. Add it to GGML_TYPES from ggml.h."
            % ggml_type
        )
    _, block_elements, block_bytes = GGML_TYPES[ggml_type]
    if n_elements % block_elements != 0:
        raise RooflineError(
            "tensor of %d elements is not a whole number of %s blocks"
            % (n_elements, GGML_TYPES[ggml_type][0])
        )
    return n_elements // block_elements * block_bytes


def read_gguf_header(path):
    """Metadata and tensor inventory of one GGUF file. Reads the header only."""
    with open(path, "rb") as handle:
        reader = _Reader(handle)
        magic, version, tensor_count, kv_count = struct.unpack(
            "<IIQQ", reader.take(24)
        )
        if magic != GGUF_MAGIC:
            raise RooflineError("%s is not a GGUF file" % path)
        if version not in (2, 3):
            raise RooflineError("unsupported GGUF version %d in %s" % (version, path))

        metadata = {}
        for _ in range(kv_count):
            key = reader.string()
            value_type = struct.unpack("<I", reader.take(4))[0]
            metadata[key] = reader.value(value_type)

        tensors = []
        for _ in range(tensor_count):
            name = reader.string()
            n_dims = struct.unpack("<I", reader.take(4))[0]
            dims = struct.unpack("<%dQ" % n_dims, reader.take(8 * n_dims))
            ggml_type = struct.unpack("<I", reader.take(4))[0]
            struct.unpack("<Q", reader.take(8))[0]  # data offset, unused here
            n_elements = 1
            for dim in dims:
                n_elements *= dim
            tensors.append(
                {
                    "name": name,
                    "dims": list(dims),
                    "ggml_type": ggml_type,
                    "n_elements": n_elements,
                    "bytes": tensor_bytes(n_elements, ggml_type),
                }
            )
        header_end = handle.tell()

    alignment = metadata.get("general.alignment", 32)
    data_start = int(math.ceil(header_end / alignment) * alignment)
    return {
        "path": path,
        "version": version,
        "metadata": metadata,
        "tensors": tensors,
        "data_start": data_start,
        "file_bytes": os.path.getsize(path),
    }


def shard_paths(path):
    """Every shard of a split model, in order. A single file yields itself."""
    match = SHARD_RE.match(os.path.basename(path))
    if not match:
        return [path]
    pattern = os.path.join(
        os.path.dirname(os.path.abspath(path)),
        "%s-*-of-%s.gguf" % (match.group("stem"), match.group("total")),
    )
    found = sorted(glob.glob(pattern))
    return found or [path]


def _meta_get(metadata, arch, suffix, default=None):
    return metadata.get("%s.%s" % (arch, suffix), default)


def describe_model(path):
    """Everything the roofline needs about one model, from its header alone."""
    shards = shard_paths(path)
    first = read_gguf_header(shards[0])
    metadata = first["metadata"]
    arch = metadata.get("general.architecture")
    if not arch:
        raise RooflineError("%s has no general.architecture" % shards[0])

    tensors = list(first["tensors"])
    header_bytes_total = first["data_start"]
    file_bytes_total = first["file_bytes"]
    for extra in shards[1:]:
        other = read_gguf_header(extra)
        tensors.extend(other["tensors"])
        header_bytes_total += other["data_start"]
        file_bytes_total += other["file_bytes"]

    expert_count = int(_meta_get(metadata, arch, "expert_count", 0) or 0)
    expert_used = int(_meta_get(metadata, arch, "expert_used_count", 0) or 0)
    has_output_head = any(t["name"] == "output.weight" for t in tensors)

    # Hybrid architectures give only some blocks an attention KV cache and put
    # the rest on a recurrent state. Which is which is read off the tensor
    # inventory rather than off an architecture-specific metadata key, so a new
    # hybrid needs no change here. Qwen3.6-35B-A3B is one: 10 attention blocks
    # out of 40, the other 30 recurrent.
    attn_blocks = set()
    recurrent_blocks = set()
    for tensor in tensors:
        match = BLOCK_RE.match(tensor["name"])
        if not match:
            continue
        index = int(match.group("index"))
        suffix = match.group("suffix")
        if suffix.startswith("attn_k"):
            attn_blocks.add(index)
        elif suffix.startswith("ssm_"):
            recurrent_blocks.add(index)

    params_total = 0
    bytes_total = 0
    expert_params = 0
    expert_bytes = 0
    embedding_params = 0
    embedding_bytes = 0
    for tensor in tensors:
        params_total += tensor["n_elements"]
        bytes_total += tensor["bytes"]
        if "_exps" in tensor["name"]:
            expert_params += tensor["n_elements"]
            expert_bytes += tensor["bytes"]
        elif tensor["name"] == "token_embd.weight":
            embedding_params += tensor["n_elements"]
            embedding_bytes += tensor["bytes"]

    # Active per token. Experts contribute only the fraction routed to.
    # The embedding table is a row lookup during decode, so its bytes are not
    # read per token, unless it doubles as the output head (weight tying), in
    # which case the whole matrix is multiplied every token.
    if expert_count > 0 and expert_used > 0:
        expert_fraction = float(expert_used) / float(expert_count)
    else:
        expert_fraction = 1.0
    dense_params = params_total - expert_params - embedding_params
    dense_bytes = bytes_total - expert_bytes - embedding_bytes
    tied = not has_output_head
    params_active = dense_params + expert_params * expert_fraction
    bytes_active = dense_bytes + expert_bytes * expert_fraction
    if tied:
        params_active += embedding_params
        bytes_active += embedding_bytes

    n_layer = int(_meta_get(metadata, arch, "block_count", 0) or 0)
    n_embd = int(_meta_get(metadata, arch, "embedding_length", 0) or 0)
    n_head = int(_meta_get(metadata, arch, "attention.head_count", 0) or 0)
    head_count_kv = _meta_get(metadata, arch, "attention.head_count_kv", n_head)
    head_dim_default = (n_embd // n_head) if n_head else 0
    key_length = int(
        _meta_get(metadata, arch, "attention.key_length", head_dim_default) or 0
    )
    value_length = int(
        _meta_get(metadata, arch, "attention.value_length", head_dim_default) or 0
    )

    n_layer_attn = len(attn_blocks) if attn_blocks else n_layer
    n_layer_recurrent = len(recurrent_blocks)

    # head_count_kv is a per-layer array on architectures with mixed attention.
    if isinstance(head_count_kv, list):
        kv_heads_summed = sum(int(v) for v in head_count_kv)
        kv_heads_display = "per-layer array, sum %d" % kv_heads_summed
    else:
        kv_heads_summed = int(head_count_kv) * n_layer_attn
        kv_heads_display = str(int(head_count_kv))

    return {
        "path": path,
        "shards": shards,
        "arch": arch,
        "name": metadata.get("general.name", os.path.basename(path)),
        "params_total": params_total,
        "params_active_per_token": int(round(params_active)),
        "bytes_total": bytes_total,
        "bytes_active_per_token": int(round(bytes_active)),
        "bits_per_weight": (bytes_total * 8.0 / params_total) if params_total else None,
        "expert_count": expert_count,
        "expert_used_count": expert_used,
        "is_moe": expert_count > 0,
        "tied_embeddings": tied,
        "n_layer": n_layer,
        "n_layer_attn": n_layer_attn,
        "n_layer_recurrent": n_layer_recurrent,
        "is_hybrid": n_layer_recurrent > 0,
        "ssm_d_conv": int(_meta_get(metadata, arch, "ssm.conv_kernel", 0) or 0),
        "ssm_d_inner": int(_meta_get(metadata, arch, "ssm.inner_size", 0) or 0),
        "ssm_d_state": int(_meta_get(metadata, arch, "ssm.state_size", 0) or 0),
        "ssm_n_group": int(_meta_get(metadata, arch, "ssm.group_count", 0) or 0),
        "n_embd": n_embd,
        "n_head": n_head,
        "kv_heads_summed": kv_heads_summed,
        "kv_heads_display": kv_heads_display,
        "key_length": key_length,
        "value_length": value_length,
        "sliding_window": _meta_get(metadata, arch, "attention.sliding_window"),
        "train_ctx": _meta_get(metadata, arch, "context_length"),
        "file_bytes": file_bytes_total,
        "header_bytes": header_bytes_total,
        "tensor_count": len(tensors),
    }


# ---------------------------------------------------------------------------
# The ceilings
# ---------------------------------------------------------------------------

KV_PAD = 256  # llama.cpp pads the cell count: llama-context.cpp, GGML_PAD(n_ctx, 256)


def kv_cache_bytes(model, n_ctx, cache_type_k="f16", cache_type_v="f16"):
    """KV cache size at a context length, matching llama.cpp's allocation.

    cells * sum_over_layers(n_head_kv) * (key_length * bytes_k + value_length *
    bytes_v). The cell count is the context padded up to 256. The cache is
    unified across slots, so it does not scale with -np.
    """
    for name in (cache_type_k, cache_type_v):
        if name not in CACHE_TYPES:
            raise RooflineError(
                "unknown cache type '%s'. Known: %s"
                % (name, ", ".join(sorted(CACHE_TYPES)))
            )
    cells = int(math.ceil(n_ctx / float(KV_PAD)) * KV_PAD)
    per_cell = (
        model["key_length"] * CACHE_TYPES[cache_type_k]
        + model["value_length"] * CACHE_TYPES[cache_type_v]
    )
    return int(round(cells * model["kv_heads_summed"] * per_cell))


def recurrent_state_bytes(model, n_seq=1):
    """Recurrent state of a hybrid model, which the KV cache does not cover.

    Two tensors per recurrent block, both f32 in llama.cpp:
      conv state  (d_inner + 2 * n_group * d_state) * (d_conv - 1)
      ssm state   d_inner * d_state
    Constant in context length, but read on every decoded token, so it belongs
    in the decode traffic. It scales with the number of sequences, unlike the
    unified KV cache.
    """
    if not model["is_hybrid"]:
        return 0
    conv_elements = (
        model["ssm_d_inner"] + 2 * model["ssm_n_group"] * model["ssm_d_state"]
    ) * max(0, model["ssm_d_conv"] - 1)
    state_elements = model["ssm_d_inner"] * model["ssm_d_state"]
    return int(model["n_layer_recurrent"] * n_seq * (conv_elements + state_elements) * 4)


def compute_roofline(model, machine, n_ctx, cache_type_k, cache_type_v, n_parallel):
    """The roofline block of the measurement record, README section 1.3.10."""
    kv_bytes = kv_cache_bytes(model, n_ctx, cache_type_k, cache_type_v)
    # One sequence: a decoding token reads its own recurrent state, not the
    # states of the other slots.
    recurrent_bytes = recurrent_state_bytes(model, 1)
    bytes_per_token = model["bytes_active_per_token"] + kv_bytes + recurrent_bytes
    bandwidth = machine["bandwidth_bytes_per_s"]
    peak_flops = machine["peak_flops"]

    decode_ceiling = bandwidth / bytes_per_token
    flops_per_token = 2.0 * model["params_active_per_token"]
    prefill_ceiling = (peak_flops / flops_per_token) if peak_flops else None

    assumptions = [
        "Decode is memory-bandwidth bound: every active weight plus the whole "
        "KV cache is read once per generated token.",
        "The KV cache is assumed full at n_ctx = %d, which is the worst case. "
        "The ceiling is higher at lower occupancy, see the context table."
        % n_ctx,
        "Attention FLOPs are not modelled beyond the linear KV traffic term, "
        "so the prefill ceiling ignores the quadratic cost of long prompts and "
        "is optimistic there. This is a stated non-goal of the calculator.",
        "Prefill FLOPs per token are 2 * params_active_per_token, counting one "
        "multiply and one add per weight. Non-matmul work is ignored.",
        "Memory bandwidth is the theoretical peak. Apple Silicon sustains "
        "roughly 70 to 85 percent of it, so a decode efficiency near 80 "
        "percent means the model is already bus-bound.",
        "Peak FLOP/s: %s" % machine["sources"]["peak_flops"],
        "Memory bandwidth: %s" % machine["sources"]["bandwidth_bytes_per_s"],
        "Bytes per weight come from the ggml block layout of each tensor, not "
        "from general.file_type, so a mixed quantisation is exact.",
        "The embedding table is %s in bytes_per_token_read: the model %s a "
        "separate output head."
        % (
            "counted" if model["tied_embeddings"] else "excluded",
            "has no" if model["tied_embeddings"] else "has",
        ),
        "The KV cache is unified across slots in current llama-server builds, "
        "so its size does not scale with -np.",
    ]
    if model["is_moe"]:
        assumptions.append(
            "Mixture of experts: %d of %d experts are read per token, so active "
            "parameters are %.1f percent of total. A batch wide enough to touch "
            "every expert reads more than this."
            % (
                model["expert_used_count"],
                model["expert_count"],
                100.0 * model["params_active_per_token"] / model["params_total"],
            )
        )
    if model["is_hybrid"]:
        assumptions.append(
            "Hybrid attention: only %d of %d blocks hold a KV cache, the other "
            "%d carry a recurrent state of %s per sequence which is constant in "
            "context length and read on every token. Both terms are in "
            "bytes_per_token_read."
            % (
                model["n_layer_attn"],
                model["n_layer"],
                model["n_layer_recurrent"],
                human_bytes(recurrent_bytes),
            )
        )
    if model["sliding_window"]:
        assumptions.append(
            "This model declares a sliding window of %s. llama.cpp allocates a "
            "smaller cache for the windowed layers, so the KV figure here is an "
            "upper bound." % model["sliding_window"]
        )

    return {
        "params_total": model["params_total"],
        "params_active_per_token": model["params_active_per_token"],
        "bits_per_weight": model["bits_per_weight"],
        "bytes_per_token_read": bytes_per_token,
        "kv_cache_bytes": kv_bytes,
        "recurrent_state_bytes": recurrent_bytes,
        "bandwidth_bytes_per_s": bandwidth,
        "peak_flops": peak_flops,
        "decode_ceiling_tps": decode_ceiling,
        "prefill_ceiling_tps": prefill_ceiling,
        "decode_efficiency_pct": None,
        "prefill_efficiency_pct": None,
        "sources": machine["sources"],
        "assumptions": assumptions,
    }


# ---------------------------------------------------------------------------
# Measured records
# ---------------------------------------------------------------------------

MEASURING_PROBES = ("speed", "prefill_curve", "agentic")


def load_measured(path):
    """Records from a campaign container or a JSON Lines trace, README 1.6."""
    with open(path, "r", encoding="utf-8") as handle:
        text = handle.read().strip()
    if not text:
        return []
    if text.startswith("{") and '"records"' in text:
        payload = json.loads(text)
        return payload.get("records", [])
    if text.startswith("["):
        return json.loads(text)
    records = []
    for line in text.splitlines():
        line = line.strip()
        if line:
            records.append(json.loads(line))
    return records


def measured_rates(records, label, n_ctx):
    """Median prefill and decode rate for a label, matched on context length.

    Reads only names defined by the contract: stats.<field>.median, where
    <field> is prefill_tps or decode_tps. A record whose engine.n_ctx differs is
    not comparable to this ceiling and is skipped rather than divided.
    """
    prefill = None
    decode = None
    matched = 0
    for record in records:
        if record.get("probe") not in MEASURING_PROBES:
            continue
        if label is not None and record.get("label") != label:
            continue
        engine = record.get("engine") or {}
        if n_ctx is not None and engine.get("n_ctx") not in (None, n_ctx):
            continue
        stats = record.get("stats") or {}
        matched += 1
        if prefill is None:
            prefill = (stats.get("prefill_tps") or {}).get("median")
        if decode is None:
            decode = (stats.get("decode_tps") or {}).get("median")
    return {"prefill_tps": prefill, "decode_tps": decode, "records_matched": matched}


def apply_efficiency(roofline, measured):
    """measured over ceiling as a percentage. Unknown stays null, never zero."""
    prefill = measured.get("prefill_tps")
    decode = measured.get("decode_tps")
    if decode is not None and roofline["decode_ceiling_tps"]:
        roofline["decode_efficiency_pct"] = 100.0 * decode / roofline["decode_ceiling_tps"]
    if prefill is not None and roofline["prefill_ceiling_tps"]:
        roofline["prefill_efficiency_pct"] = (
            100.0 * prefill / roofline["prefill_ceiling_tps"]
        )
    return roofline


# ---------------------------------------------------------------------------
# run-matrix.sh
# ---------------------------------------------------------------------------

MB_RE = re.compile(r'^MB=(?P<value>.+?)\s*$', re.M)
ROW_RE = re.compile(r'^\s*"(?P<row>[^"|]+\|[^"]+)"\s*$', re.M)


def expand_shell_value(raw):
    """Expand the small subset of shell syntax used by run-matrix.sh."""
    raw = raw.strip().strip('"').strip("'")
    match = re.match(r"^\$\{(?P<name>\w+):-(?P<fallback>[^}]*)\}$", raw)
    if match:
        raw = os.environ.get(match.group("name")) or match.group("fallback")
    raw = raw.replace("$HOME", os.path.expanduser("~"))
    return os.path.expanduser(raw)


def read_run_matrix(models_dir=None):
    """Rows of run-matrix.sh, so the two lists never drift apart."""
    with open(RUN_MATRIX, "r", encoding="utf-8") as handle:
        text = handle.read()
    mb_match = MB_RE.search(text)
    if not mb_match:
        raise RooflineError("no MB= assignment found in %s" % RUN_MATRIX)
    base = models_dir or expand_shell_value(mb_match.group("value"))
    rows = []
    for match in ROW_RE.finditer(text):
        parts = match.group("row").split("|")
        if len(parts) != 4:
            continue
        label, pattern, n_parallel, n_ctx = parts
        pattern = pattern.replace("$MB", base)
        found = sorted(glob.glob(pattern))
        rows.append(
            {
                "label": label,
                "pattern": pattern,
                "gguf": found[0] if found else None,
                "n_parallel": int(n_parallel),
                "n_ctx": int(n_ctx),
            }
        )
    return rows


# ---------------------------------------------------------------------------
# KV validation against llama-server
# ---------------------------------------------------------------------------

# llama_kv_cache: size = 544.00 MiB ( 4096 cells, 34 layers, 4/1 seqs),
#                 K (f16): 272.00 MiB, V (f16): 272.00 MiB
KV_LINE_RE = re.compile(
    r"llama_kv_cache\w*:\s*size\s*=\s*(?P<total>[\d.]+)\s*MiB\s*\(\s*"
    r"(?P<cells>\d+)\s*cells,\s*(?P<layers>\d+)\s*layers,\s*"
    r"(?P<seq_max>\d+)/(?P<streams>\d+)\s*seqs\),\s*"
    r"K\s*\((?P<ktype>[^)]+)\):\s*(?P<kmib>[\d.]+)\s*MiB,\s*"
    r"V\s*\((?P<vtype>[^)]+)\):\s*(?P<vmib>[\d.]+)\s*MiB"
)
# Older builds: "llama_kv_cache_unified: KV self size = 544.00 MiB, K (f16) ..."
KV_LINE_LEGACY_RE = re.compile(
    r"KV self size\s*=\s*(?P<total>[\d.]+)\s*MiB.*?"
    r"K\s*\((?P<ktype>[^)]+)\):\s*(?P<kmib>[\d.]+)\s*MiB.*?"
    r"V\s*\((?P<vtype>[^)]+)\):\s*(?P<vmib>[\d.]+)\s*MiB"
)
# llama_memory_recurrent: size = 251.25 MiB ( 4 cells, 40 layers, 4 seqs ...)
RS_LINE_RE = re.compile(
    r"llama_memory_recurrent:\s*size\s*=\s*(?P<total>[\d.]+)\s*MiB\s*\(\s*"
    r"(?P<cells>\d+)\s*cells"
)

LLAMA_SERVER_TIMEOUT_S = 300.0


def locate_llama_server(explicit=None):
    """The binary to interrogate, resolved the way the runtime resolves it."""
    if explicit:
        return explicit
    from_env = os.environ.get("APOLLIA_LLAMA_SERVER_BIN")
    if from_env and os.path.isfile(from_env):
        return from_env
    for directory in os.environ.get("PATH", "").split(os.pathsep):
        candidate = os.path.join(directory, "llama-server")
        if os.path.isfile(candidate) and os.access(candidate, os.X_OK):
            return candidate
    for candidate in ("/opt/homebrew/bin/llama-server", "/usr/local/bin/llama-server"):
        if os.path.isfile(candidate):
            return candidate
    raise RooflineError("llama-server not found. Pass --llama-server.")


def probe_kv_allocation(
    binary, gguf, n_ctx, cache_type_k, cache_type_v, port, expect_recurrent=False
):
    """Launch llama-server, read its reported cache allocation, stop it.

    Verbosity 5 is required: the cache lines are not emitted at the default
    level in recent builds.
    """
    args = [
        binary,
        "-m", gguf,
        "-ngl", "999",
        "-c", str(n_ctx),
        "--no-warmup",
        "-lv", "5",
        "--host", "127.0.0.1",
        "--port", str(port),
    ]
    if cache_type_k != "f16":
        args += ["-ctk", cache_type_k]
    if cache_type_v != "f16":
        args += ["-ctv", cache_type_v]

    process = subprocess.Popen(
        args,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        bufsize=1,
        universal_newlines=True,
        errors="replace",
    )
    deadline = time.monotonic() + LLAMA_SERVER_TIMEOUT_S
    found = None
    recurrent = None
    try:
        for line in process.stderr:
            match = KV_LINE_RE.search(line) or KV_LINE_LEGACY_RE.search(line)
            if match and found is None:
                fields = match.groupdict()
                found = {
                    "reported_bytes": int(round(float(fields["total"]) * MIB)),
                    "k_bytes": int(round(float(fields["kmib"]) * MIB)),
                    "v_bytes": int(round(float(fields["vmib"]) * MIB)),
                    "cells": int(fields["cells"]) if "cells" in fields else None,
                    "layers": int(fields["layers"]) if "layers" in fields else None,
                    "cache_type_k": fields["ktype"].strip(),
                    "cache_type_v": fields["vtype"].strip(),
                    "launch_args": args,
                }
            rs_match = RS_LINE_RE.search(line)
            if rs_match and recurrent is None:
                recurrent = {
                    "reported_bytes": int(round(float(rs_match.group("total")) * MIB)),
                    "cells": int(rs_match.group("cells")),
                }
            if found is not None and (recurrent is not None or not expect_recurrent):
                break
            if time.monotonic() > deadline:
                raise RooflineError(
                    "llama-server did not report a cache allocation within %.0f s"
                    % LLAMA_SERVER_TIMEOUT_S
                )
    finally:
        process.terminate()
        try:
            process.wait(timeout=15)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=15)
    if found is None:
        raise RooflineError(
            "llama-server exited without reporting a cache allocation for %s"
            % os.path.basename(gguf)
        )
    found["recurrent"] = recurrent
    return found


# ---------------------------------------------------------------------------
# Provenance
# ---------------------------------------------------------------------------


def run_capture(args, merge_stderr=False):
    """Command output, or None when the command is unavailable or fails."""
    try:
        result = subprocess.run(
            args, capture_output=True, text=True, timeout=30, check=False
        )
    except (OSError, subprocess.SubprocessError):
        return None
    if result.returncode != 0:
        return None
    # llama-server writes --version to stderr.
    return (result.stdout + result.stderr) if merge_stderr else result.stdout


def build_provenance(model_path, llama_server):
    """The provenance block of README section 1.3.8. R5: no number without it."""
    git_sha = run_capture(["git", "-C", REPO_ROOT, "rev-parse", "HEAD"])
    git_status = run_capture(["git", "-C", REPO_ROOT, "status", "--porcelain"])
    version = None
    path = None
    try:
        path = locate_llama_server(llama_server)
        raw = run_capture([path, "--version"], merge_stderr=True)
        if raw:
            version = raw.strip().splitlines()[0]
    except RooflineError:
        pass
    memory = run_capture(["sysctl", "-n", "hw.memsize"])
    return {
        "git_sha": git_sha.strip() if git_sha else "unknown",
        "git_dirty": bool(git_status and git_status.strip()),
        "llama_server_version": version or "unknown",
        "llama_server_path": path or "unknown",
        "model_path": model_path,
        # Hashing a 20 GiB file on every roofline run is not worth its cost, and
        # the roofline reads no tensor data, so the header alone determines the
        # result. I7: absent is null with the reason recorded in notes.
        "model_sha256": None,
        "model_sha256_scope": "none",
        "launch_args": [],
        "machine_id": (detect_machine_id() or "unknown"),
        "machine_chip": (run_capture(["sysctl", "-n", "machdep.cpu.brand_string"]) or "unknown").strip(),
        "machine_memory_bytes": int(memory.strip()) if memory else 0,
        "os_version": (run_capture(["sw_vers", "-productVersion"]) or "unknown").strip(),
    }


def build_conditions(note):
    """Run conditions, README 1.3.9. A computed record samples nothing."""
    return {
        "run_index": 0,
        "run_order": "sequential",
        "page_cache": "unknown",
        "server_restarted_before": False,
        "slot_reset_before": False,
        "sampling_seed": -1,
        "sampling_temperature": 0.0,
        "notes": note,
    }


def now_rfc3339():
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())


def build_record(label, model, roofline, n_ctx, n_parallel, cache_k, cache_v, llama_server):
    """One measurement record with probe == "roofline", README section 1.5."""
    return {
        "schema_version": 1,
        "record_id": "roofline-%s-ctx%d" % (label, n_ctx),
        "campaign_id": None,
        "probe": "roofline",
        "label": label,
        "measured_at": now_rfc3339(),
        "provenance": build_provenance(model["path"], llama_server),
        "conditions": build_conditions(
            "computed record, nothing was sampled. model_sha256 is null because "
            "the roofline reads the GGUF header only and never the tensor data."
        ),
        "engine": {
            "n_ctx": n_ctx,
            "n_ctx_slot_tok": n_ctx // max(1, n_parallel),
            "n_gpu_layers": 999,
            "n_parallel": n_parallel,
            "cache_type_k": cache_k,
            "cache_type_v": cache_v,
        },
        "roofline": roofline,
        "invalid": [],
    }


# ---------------------------------------------------------------------------
# Rendering
# ---------------------------------------------------------------------------

CONTEXT_LADDER = (0, 4096, 16384, 32768, 65536)


def human_count(value):
    if value is None:
        return "n/a"
    if value >= 1e9:
        return "%.2f G" % (value / 1e9)
    if value >= 1e6:
        return "%.2f M" % (value / 1e6)
    return str(int(value))


def human_bytes(value):
    if value is None:
        return "n/a"
    if value >= GIB:
        return "%.2f GiB" % (value / GIB)
    return "%.1f MiB" % (value / MIB)


def render_model(model, machine, roofline, n_ctx, cache_k, cache_v, measured, out):
    active_pct = 100.0 * model["params_active_per_token"] / model["params_total"]
    out.append("MODEL  %s" % model["name"])
    out.append("  file             %s" % model["path"])
    if len(model["shards"]) > 1:
        out.append("  shards           %d" % len(model["shards"]))
    out.append(
        "  architecture     %-14s layers %-5d hidden %d"
        % (model["arch"], model["n_layer"], model["n_embd"])
    )
    if model["is_hybrid"]:
        out.append(
            "  hybrid           %d attention blocks, %d recurrent blocks"
            % (model["n_layer_attn"], model["n_layer_recurrent"])
        )
    out.append(
        "  attention        %d heads / %s kv heads, head dim %d k / %d v"
        % (model["n_head"], model["kv_heads_display"], model["key_length"], model["value_length"])
    )
    if model["is_moe"]:
        out.append(
            "  experts          %d used of %d"
            % (model["expert_used_count"], model["expert_count"])
        )
    out.append(
        "  parameters       %s total, %s active per token (%.1f %%)"
        % (
            human_count(model["params_total"]),
            human_count(model["params_active_per_token"]),
            active_pct,
        )
    )
    out.append(
        "  weights          %s total, %s read per token, %.2f bits/weight"
        % (
            human_bytes(model["bytes_total"]),
            human_bytes(model["bytes_active_per_token"]),
            model["bits_per_weight"],
        )
    )
    # Self-check on the whole ggml type table at once: tensor bytes plus headers
    # must account for the files on disk.
    accounted = model["bytes_total"] + model["header_bytes"]
    drift = abs(accounted - model["file_bytes"]) / float(model["file_bytes"])
    out.append(
        "  size check       %s computed vs %s on disk (%.3f %% drift)"
        % (human_bytes(accounted), human_bytes(model["file_bytes"]), 100.0 * drift)
    )
    if drift > 0.01:
        out.append("  WARNING          computed size disagrees with the file by over 1 %")

    out.append("")
    out.append(
        "CEILINGS on %s at n_ctx = %d, KV %s/%s"
        % (machine["chip"], n_ctx, cache_k, cache_v)
    )
    out.append(
        "  bandwidth        %.1f GB/s        peak %s"
        % (
            machine["bandwidth_bytes_per_s"] / 1e9,
            ("%.1f TFLOP/s FP16" % (machine["peak_flops"] / 1e12))
            if machine["peak_flops"]
            else "n/a",
        )
    )
    out.append("  kv cache         %s" % human_bytes(roofline["kv_cache_bytes"]))
    if roofline["recurrent_state_bytes"]:
        out.append(
            "  recurrent state  %s per sequence"
            % human_bytes(roofline["recurrent_state_bytes"])
        )
    out.append(
        "  bytes per token  %s  (weights %s + kv %s%s)"
        % (
            human_bytes(roofline["bytes_per_token_read"]),
            human_bytes(model["bytes_active_per_token"]),
            human_bytes(roofline["kv_cache_bytes"]),
            " + state %s" % human_bytes(roofline["recurrent_state_bytes"])
            if roofline["recurrent_state_bytes"]
            else "",
        )
    )
    out.append("  decode ceiling   %.1f tok/s" % roofline["decode_ceiling_tps"])
    out.append(
        "  prefill ceiling  %s"
        % (
            "%.0f tok/s" % roofline["prefill_ceiling_tps"]
            if roofline["prefill_ceiling_tps"]
            else "n/a, no peak FLOP/s for this machine"
        )
    )

    out.append("")
    out.append("  decode ceiling by context occupancy")
    out.append("    %-10s %-14s %s" % ("n_ctx", "kv cache", "decode ceiling"))
    state = roofline["recurrent_state_bytes"]
    for ctx in CONTEXT_LADDER:
        kv = kv_cache_bytes(model, ctx, cache_k, cache_v) if ctx else 0
        ceiling = machine["bandwidth_bytes_per_s"] / (
            model["bytes_active_per_token"] + kv + state
        )
        out.append(
            "    %-10d %-14s %.1f tok/s" % (ctx, human_bytes(kv) if kv else "0", ceiling)
        )

    if measured is not None:
        out.append("")
        out.append("EFFICIENCY, measured over ceiling")
        out.append(
            "    %-10s %-14s %-14s %s"
            % ("quantity", "measured", "ceiling", "efficiency")
        )
        for name, meas_key, ceil_key, eff_key in (
            ("prefill", "prefill_tps", "prefill_ceiling_tps", "prefill_efficiency_pct"),
            ("decode", "decode_tps", "decode_ceiling_tps", "decode_efficiency_pct"),
        ):
            measured_value = measured.get(meas_key)
            ceiling_value = roofline[ceil_key]
            efficiency = roofline[eff_key]
            out.append(
                "    %-10s %-14s %-14s %s"
                % (
                    name,
                    "%.1f tok/s" % measured_value if measured_value is not None else "n/a",
                    "%.1f tok/s" % ceiling_value if ceiling_value is not None else "n/a",
                    "%.1f %%" % efficiency if efficiency is not None else "n/a",
                )
            )
        if measured.get("records_matched") == 0:
            out.append(
                "    no record in the measurement file matches this label and "
                "context length"
            )


def render_assumptions(roofline, out):
    out.append("")
    out.append("ASSUMPTIONS")
    for line in roofline["assumptions"]:
        wrapped = wrap(line, 74)
        out.append("  - %s" % wrapped[0])
        for extra in wrapped[1:]:
            out.append("    %s" % extra)


def wrap(text, width):
    words = text.split()
    lines = []
    current = ""
    for word in words:
        if current and len(current) + 1 + len(word) > width:
            lines.append(current)
            current = word
        else:
            current = "%s %s" % (current, word) if current else word
    if current:
        lines.append(current)
    return lines or [""]


# ---------------------------------------------------------------------------
# Modes
# ---------------------------------------------------------------------------


def label_for(path):
    return os.path.splitext(os.path.basename(path))[0]


def mode_single(args, machine, records, out):
    model = describe_model(args.gguf)
    n_ctx = args.ctx[0]
    roofline = compute_roofline(
        model, machine, n_ctx, args.cache_type_k, args.cache_type_v, args.n_parallel
    )
    label = args.label or label_for(args.gguf)
    measured = None
    if records is not None:
        measured = measured_rates(records, label, n_ctx)
        apply_efficiency(roofline, measured)
    render_model(
        model, machine, roofline, n_ctx, args.cache_type_k, args.cache_type_v, measured, out
    )
    render_assumptions(roofline, out)
    return [
        build_record(
            label,
            model,
            roofline,
            n_ctx,
            args.n_parallel,
            args.cache_type_k,
            args.cache_type_v,
            args.llama_server,
        )
    ]


def mode_matrix(args, machine, records, out):
    rows = read_run_matrix(args.models_dir)
    out.append("MATRIX  %d rows from %s" % (len(rows), RUN_MATRIX))
    out.append("")
    header = "%-24s %10s %10s %7s %9s %9s %8s %8s" % (
        "label", "params", "active", "bpw", "kv", "decode", "prefill", "dec eff",
    )
    out.append(header)
    out.append("-" * len(header))
    built = []
    missing = []
    assumptions_shown = None
    for row in rows:
        if row["gguf"] is None:
            missing.append(row)
            out.append("%-24s %s" % (row["label"], "MISSING"))
            continue
        model = describe_model(row["gguf"])
        n_ctx = args.ctx[0] if args.ctx_explicit else row["n_ctx"]
        roofline = compute_roofline(
            model, machine, n_ctx, args.cache_type_k, args.cache_type_v, row["n_parallel"]
        )
        if records is not None:
            apply_efficiency(roofline, measured_rates(records, row["label"], n_ctx))
        out.append(
            "%-24s %10s %10s %7.2f %9s %6.1f/s %6.0f/s %8s"
            % (
                row["label"],
                human_count(model["params_total"]),
                human_count(model["params_active_per_token"]),
                model["bits_per_weight"],
                human_bytes(roofline["kv_cache_bytes"]),
                roofline["decode_ceiling_tps"],
                roofline["prefill_ceiling_tps"] or 0.0,
                "%.1f %%" % roofline["decode_efficiency_pct"]
                if roofline["decode_efficiency_pct"] is not None
                else "n/a",
            )
        )
        built.append(
            build_record(
                row["label"],
                model,
                roofline,
                n_ctx,
                row["n_parallel"],
                args.cache_type_k,
                args.cache_type_v,
                args.llama_server,
            )
        )
        assumptions_shown = roofline
    for row in missing:
        out.append("")
        out.append("MISSING  %s, no file matches %s" % (row["label"], row["pattern"]))
    if assumptions_shown is not None:
        render_assumptions(assumptions_shown, out)
    return built


def mode_validate_kv(args, out):
    """Check the KV formula against llama-server's own reported allocation."""
    binary = locate_llama_server(args.llama_server)
    version = run_capture([binary, "--version"], merge_stderr=True)
    out.append("KV CACHE VALIDATION")
    out.append("  binary   %s" % binary)
    out.append("  version  %s" % (version.strip().splitlines()[0] if version else "unknown"))
    out.append("")
    header = "%-26s %-10s %7s %7s %12s %12s %9s" % (
        "model", "pool", "n_ctx", "cells", "predicted", "reported", "delta",
    )
    out.append(header)
    out.append("-" * len(header))

    worst = 0.0
    failures = 0
    for gguf in args.validate_gguf:
        model = describe_model(gguf)
        name = os.path.basename(gguf)[:26]
        for n_ctx in args.ctx:
            reported = probe_kv_allocation(
                binary,
                gguf,
                n_ctx,
                args.cache_type_k,
                args.cache_type_v,
                args.port,
                expect_recurrent=model["is_hybrid"],
            )
            pools = [
                (
                    "kv cache",
                    kv_cache_bytes(model, n_ctx, args.cache_type_k, args.cache_type_v),
                    reported["reported_bytes"],
                    reported["cells"],
                )
            ]
            if reported["recurrent"] is not None:
                # The recurrent pool is allocated once per slot, so it is
                # predicted at the slot count the engine actually chose.
                pools.append(
                    (
                        "recurrent",
                        recurrent_state_bytes(model, reported["recurrent"]["cells"]),
                        reported["recurrent"]["reported_bytes"],
                        reported["recurrent"]["cells"],
                    )
                )
            for pool, predicted, actual, cells in pools:
                delta = abs(predicted - actual) / float(actual) if actual else 0.0
                worst = max(worst, delta)
                if delta > args.tolerance:
                    failures += 1
                out.append(
                    "%-26s %-10s %7d %7s %12s %12s %8.3f %%"
                    % (
                        name,
                        pool,
                        n_ctx,
                        str(cells) if cells else "n/a",
                        human_bytes(predicted),
                        human_bytes(actual),
                        100.0 * delta,
                    )
                )
    out.append("")
    out.append("  worst disagreement %.3f %%, tolerance %.3f %%" % (100.0 * worst, 100.0 * args.tolerance))
    out.append(
        "  verdict %s"
        % ("AGREES" if failures == 0 else "DISAGREES on %d pair(s)" % failures)
    )
    return failures


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------


def parse_args(argv):
    parser = argparse.ArgumentParser(
        description="Theoretical ceilings for a GGUF model on a given machine."
    )
    parser.add_argument("--gguf", help="path to the GGUF, or its first shard")
    parser.add_argument("--label", help="model label, defaults to the file stem")
    parser.add_argument(
        "--matrix", action="store_true", help="every model declared in run-matrix.sh"
    )
    parser.add_argument(
        "--models-dir", help="substitute the model base directory used by run-matrix.sh"
    )
    parser.add_argument(
        "--ctx",
        default="32768",
        help="context length, or a comma separated list for --validate-kv",
    )
    parser.add_argument("--n-parallel", type=int, default=1, help="slots, for n_ctx_slot_tok")
    parser.add_argument("--cache-type-k", default="f16", help="KV cache type for K")
    parser.add_argument("--cache-type-v", default="f16", help="KV cache type for V")
    parser.add_argument("--machine", help="machine identifier, defaults to this machine")
    parser.add_argument("--bandwidth", type=float, help="memory bandwidth override, bytes/s")
    parser.add_argument("--peak-flops", type=float, help="peak throughput override, FLOP/s")
    parser.add_argument(
        "--measured", help="campaign file or JSON Lines trace of measurement records"
    )
    parser.add_argument("--json", action="store_true", help="emit a campaign container")
    parser.add_argument(
        "--validate-kv",
        action="store_true",
        help="check the KV formula against llama-server's reported allocation",
    )
    parser.add_argument("--llama-server", help="llama-server binary for validation")
    parser.add_argument("--port", type=int, default=8099, help="port for validation runs")
    parser.add_argument(
        "--tolerance", type=float, default=0.01, help="allowed KV disagreement, 0.01 = 1 %%"
    )
    args = parser.parse_args(argv)
    args.ctx_explicit = "--ctx" in argv
    args.ctx = [int(part) for part in str(args.ctx).split(",") if part.strip()]
    if not args.ctx:
        parser.error("--ctx needs at least one value")
    if not args.matrix and not args.gguf:
        parser.error("pass --gguf or --matrix")
    return args


def main(argv):
    try:
        args = parse_args(argv)
        out = []

        if args.validate_kv:
            if args.matrix:
                rows = read_run_matrix(args.models_dir)
                args.validate_gguf = [r["gguf"] for r in rows if r["gguf"]]
            else:
                args.validate_gguf = [args.gguf]
            if not args.validate_gguf:
                raise RooflineError("no GGUF to validate")
            failures = mode_validate_kv(args, out)
            sys.stdout.write("\n".join(out) + "\n")
            return 1 if failures else 0

        machine = resolve_machine(args.machine, args.bandwidth, args.peak_flops)
        records = load_measured(args.measured) if args.measured else None
        if records is not None and not records:
            out.append(
                "NOTE  %s holds no measurement record, efficiency is null"
                % args.measured
            )
            out.append("")

        if args.matrix:
            built = mode_matrix(args, machine, records, out)
        else:
            built = mode_single(args, machine, records, out)

        if args.json:
            container = {
                "schema_version": 1,
                "campaign_id": "roofline-%s" % now_rfc3339(),
                "started_at": now_rfc3339(),
                "finished_at": now_rfc3339(),
                "records": built,
                "records_excluded": [],
            }
            sys.stdout.write(json.dumps(container, ensure_ascii=False, indent=1) + "\n")
        else:
            sys.stdout.write("\n".join(out) + "\n")
        return 0
    except RooflineError as exc:
        sys.stderr.write("roofline: %s\n" % exc)
        return 2
    except OSError as exc:
        sys.stderr.write("roofline: %s\n" % exc)
        return 2


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
