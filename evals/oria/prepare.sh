#!/usr/bin/env bash
# Rebuild the throwaway run workspace of the ORIA eval suite from the seed.
#
# The suite mutates its workspace (it edits a config file, it writes a script),
# so the workspace a run is played against must be rebuilt before every run,
# otherwise the second run measures the leftovers of the first.
#
# Exit codes, the same three every guard and measurement script in this tree
# uses:
#   0  the workspace was rebuilt and verified
#   1  a defect: the rebuild happened but the result does not match the seed
#   2  nothing was done: there is no seed to copy from
#
# A 2 must never be readable as a success. That is the point of separating it
# from 0.

set -u

here="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
seed="${here}/seed"
run="${here}/run"

if [ ! -d "${seed}" ]; then
  echo "prepare: no seed directory at ${seed}; nothing prepared" >&2
  exit 2
fi

seed_files="$(find "${seed}" -type f | wc -l | tr -d ' ')"
if [ "${seed_files}" -eq 0 ]; then
  echo "prepare: the seed at ${seed} holds no file; nothing prepared" >&2
  exit 2
fi

rm -rf -- "${run}"
if ! mkdir -p -- "${run}"; then
  echo "prepare: could not create ${run}" >&2
  exit 1
fi

if ! (cd -- "${seed}" && tar cf - .) | (cd -- "${run}" && tar xf -); then
  echo "prepare: copying the seed into ${run} failed" >&2
  exit 1
fi

run_files="$(find "${run}" -type f | wc -l | tr -d ' ')"
if [ "${run_files}" -ne "${seed_files}" ]; then
  echo "prepare: ${run_files} files in the workspace, ${seed_files} in the seed" >&2
  exit 1
fi

if [ ! -x "${run}/tools/collect.sh" ]; then
  echo "prepare: ${run}/tools/collect.sh is not executable; the recovery task cannot run it" >&2
  exit 1
fi

echo "prepare: ${run_files} files rebuilt at ${run}"
exit 0
