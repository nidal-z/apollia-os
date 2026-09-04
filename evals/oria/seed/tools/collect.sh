#!/usr/bin/env bash
# Deliberately failing collector, used by the recovery task of the eval suite.
# It always fails, with a stable message and a stable exit status.
echo "collect: depot endpoint unreachable (seeded failure)" >&2
exit 3
