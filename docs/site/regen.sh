#!/usr/bin/env bash
#
# Regenerate the three machine references of the Apollia docs site from the
# source of truth. Nothing here is hand-copied.
#
#   HTTP API  <- clients/openapi.json   (docusaurus-plugin-openapi-docs)
#   CLI       <- apollia-os clap tree   (clap-markdown, behind the gen-docs feature)
#   SDK / ctx <- sdk/apollia/*.py        (stdlib ast introspection)
#
# The API output under docs/reference/api is regenerated at build time and is
# gitignored. The CLI and SDK pages are committed (they need cargo / python that
# a plain node build does not have), so run this, review the diff, then commit.

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$HERE/../.." && pwd)"

HEADER="<!-- GENERATED FILE. Do not edit; regenerate with docs/site/regen.sh. -->"

echo "==> SDK / ctx reference (ast introspection of sdk/apollia)"
python3 "$HERE/scripts/gen_sdk_ref.py"

echo "==> CLI reference (apollia-os clap tree via the gen-docs feature)"
CLI_OUT="$HERE/docs/reference/cli/index.md"
mkdir -p "$(dirname "$CLI_OUT")"
{
  echo "---"
  echo "sidebar_position: 1"
  echo "title: CLI reference"
  echo "---"
  echo "$HEADER"
  # Drop clap-markdown's own top-level H1; the frontmatter title stands in.
  cargo run --quiet --manifest-path "$REPO_ROOT/Cargo.toml" \
    -p apollia-cli --features gen-docs -- gen-cli-docs | tail -n +2
} > "$CLI_OUT"
echo "    wrote $CLI_OUT"

echo "==> HTTP API reference (OpenAPI spec via docusaurus-plugin-openapi-docs)"
if [ -d "$HERE/node_modules" ]; then
  (cd "$HERE" && npm run --silent clean-api >/dev/null 2>&1 || true)
  (cd "$HERE" && npm run --silent gen-api)
  echo "    generated docs/reference/api (gitignored, rebuilt at build time)"
else
  echo "    skipped: run 'npm install' in docs/site first, or 'npm run build'"
fi

echo "==> done. Review the diff, then commit the CLI and SDK pages."
