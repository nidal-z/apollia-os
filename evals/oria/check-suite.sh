#!/usr/bin/env bash
# Load check for the ORIA agent-loop eval suite.
#
# Two things are verified, and neither of them needs a model:
#   1. the suite loads through the crate's own model and holds its invariants,
#      by running `crates/apollia-eval/tests/oria_suite_loads.rs`;
#   2. the SHA-256 pinned in the surgical-edit task still matches the digest of
#      the seed file after the single edit that task asks for. That digest is
#      the only thing standing behind the words "and the rest intact", and it
#      goes stale the moment anyone touches the seed.
#
# Exit codes:
#   0  measured, and clean
#   1  a defect: a test failed, or the pinned digest no longer matches the seed
#   2  nothing was measured: no cargo, no suite, no digest tool, or a test run
#      that executed zero tests
#
# The zero-test case is the reason 2 exists. `cargo test` with a filter that
# matches nothing exits 0, and a script that trusted that exit code would report
# a green having checked nothing at all.

set -u

here="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd -- "${here}/../.." && pwd)"
suite="${here}/agent-loop.toml"
seed_conf="${here}/seed/config/service.conf"

if [ ! -f "${suite}" ]; then
  echo "check: no suite at ${suite}; nothing measured" >&2
  exit 2
fi
if [ ! -f "${seed_conf}" ]; then
  echo "check: no seed config at ${seed_conf}; nothing measured" >&2
  exit 2
fi
if ! command -v cargo >/dev/null 2>&1; then
  echo "check: cargo is not on PATH; nothing measured" >&2
  exit 2
fi

# --- 1. the pinned digest against the seed ---------------------------------

digest_of() {
  # Prints the lowercase hex digest of stdin, or nothing when no tool is found.
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum | cut -d' ' -f1
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 | cut -d' ' -f1
  else
    return 1
  fi
}

expected="$(sed 's/^retry_limit = 3$/retry_limit = 9/' "${seed_conf}" | digest_of)"
if [ -z "${expected}" ]; then
  echo "check: no sha256 tool available; nothing measured" >&2
  exit 2
fi

pinned="$(grep -oE '[0-9a-f]{64}' "${suite}" | head -n 1)"
if [ -z "${pinned}" ]; then
  echo "check: the suite pins no sha256 digest; nothing measured" >&2
  exit 2
fi

if [ "${pinned}" != "${expected}" ]; then
  echo "check: the surgical-edit task pins ${pinned}" >&2
  echo "check: the seed after its one edit hashes to ${expected}" >&2
  echo "check: the task would blame the agent for a digest the seed can no longer produce" >&2
  exit 1
fi

# --- 2. the load test ------------------------------------------------------

log="$(mktemp -t oria-suite-load-check)"
# The measured command must not sit in a pipeline: this shell family reports the
# exit code of the last stage, not of the command under measurement.
cargo test -p apollia-eval --test oria_suite_loads >"${log}" 2>&1
status=$?

summary="$(grep -E '^test result:' "${log}" | tail -n 1)"
if [ -z "${summary}" ]; then
  echo "check: the test binary printed no result line; nothing measured" >&2
  sed -n '$p' "${log}" >&2
  rm -f -- "${log}"
  exit 2
fi

passed="$(echo "${summary}" | sed -nE 's/.* ([0-9]+) passed.*/\1/p')"
failed="$(echo "${summary}" | sed -nE 's/.* ([0-9]+) failed.*/\1/p')"
passed="${passed:-0}"
failed="${failed:-0}"

if [ "$((passed + failed))" -eq 0 ]; then
  echo "check: the test binary ran zero tests; nothing measured" >&2
  rm -f -- "${log}"
  exit 2
fi

if [ "${status}" -ne 0 ] || [ "${failed}" -ne 0 ]; then
  echo "check: ${failed} of $((passed + failed)) load invariants failed" >&2
  grep -E '^(test .*FAILED|---- |thread )' "${log}" >&2
  echo "check: full output at ${log}" >&2
  exit 1
fi

rm -f -- "${log}"
echo "check: ${passed} load invariants held, digest ${pinned} matches the seed"
echo "check: suite ${suite#"${repo}"/}"
exit 0
