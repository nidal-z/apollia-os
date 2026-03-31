#!/usr/bin/env bash
# Apollia Code Reviewer — post-commit hook
#
# Fires the apollia-code-reviewer webhook trigger after each commit.
#
# Setup:
#   cp scripts/post-commit-hook.sh .git/hooks/post-commit
#   chmod +x .git/hooks/post-commit
#
# Configuration:
#   Set APOLLIA_TRIGGER_ID to the ID of your webhook trigger.
#   Get it after creating the trigger via the Apollia UI or:
#     apollia trigger list
#
# The hook fires silently — it does not block the commit.
# Review results appear as tasks in the Apollia UI.

APOLLIA_BASE_URL="${APOLLIA_BASE_URL:-http://127.0.0.1:7771}"
APOLLIA_TRIGGER_ID="${APOLLIA_TRIGGER_ID:-}"

# If no trigger ID is configured, skip silently.
if [[ -z "$APOLLIA_TRIGGER_ID" ]]; then
  exit 0
fi

COMMIT_HASH=$(git log -1 --format="%H" 2>/dev/null)
COMMIT_MSG=$(git log -1 --format="%s" 2>/dev/null)

# Fire the trigger — fail silently so the hook never blocks a commit.
curl -s \
  --max-time 3 \
  -X POST \
  -H "Content-Type: application/json" \
  "${APOLLIA_BASE_URL}/api/v1/triggers/${APOLLIA_TRIGGER_ID}/fire" \
  -d "{\"commit\": \"${COMMIT_HASH}\", \"message\": \"${COMMIT_MSG}\"}" \
  > /dev/null 2>&1 || true

exit 0
