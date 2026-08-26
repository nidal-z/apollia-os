#!/usr/bin/env python3
"""Fail when the desktop UI writes a visual value a design token already covers.

`crates/apollia-desktop/ui/AGENTS.md` section 3 states the rule: never a
hardcoded colour, spacing, radius or shadow, always the HSL custom properties
of `src/app.css` through the `tailwind.config.ts` mapping. Nothing enforced it.
The only tool the rulebook offered was a `grep` for hex and palette classes,
which answers 347 lines on this tree, most of them `#app`, `&#9201;` and
identifiers. A rule whose only instrument cries wolf is a rule nobody runs, and
1020 literals had accumulated behind it.

The reason the rule matters is that its violations are silent. A hardcoded
`#faf6ec` renders correctly in the light theme and wrong in the dark one; no
build fails, no test fails, and the defect is only visible to someone who
toggles the theme on that exact surface.

Seven families are read, one per token family that exists:

  color      hex, `rgb()`/`hsl()` with literal numbers, Tailwind palette
             classes (`bg-white`, `text-neutral-500`) and arbitrary colour
             classes (`bg-[#fff]`)
  shadow     `box-shadow:` literals and `shadow-[...]`, against `--shadow-*`
  radius     `border-radius:` literals and `rounded-[...]`, against `--radius`
  z-index    `z-index:` literals, `z-[N]` and `z-NN`, against `--z-*`
  motion     literal durations and `cubic-bezier(...)`, against `--motion-*`
             and `--ease-*`
  font-size  `text-[Npx]` and `font-size: Npx`, against the reading and
             chrome scales of `tailwind.config.ts`
  size-px    other arbitrary px classes (`w-[12px]`, `gap-[3px]`); Tailwind's
             spacing scale is the token here

What separates this guard from the sweep it was promoted from is where it
looks. A literal is a defect where it can style something: inside a `<script>`
or `<style>` block, inside a tag in the template, or anywhere in a `.css` file.
A literal sitting in element text content is prose, and the showcase routes are
full of it, documenting the design system by naming its values. The sweep read
whole lines and reported `hsl(28 11% 13%)` written inside a `<code>` span as a
hardcoded colour. Counting prose as a defect is how a guard earns the reputation
that gets it switched off.

The ratchet. This tree does not reach zero in one change, so the debt is
carried as a named allowance per file, in `ALLOWED` below, and the ratchet only
descends: a file above its allowance fails, a file *below* its allowance also
fails, with the instruction to lower the number. An allowance that could drift
downward unnoticed would let the list outlive the debt, and a list nobody
prunes is the debt with a green light on it.

Exit codes: 0 clean, 1 at least one file off its allowance, 2 nothing measured.

Usage:
    python3 scripts/check_design_tokens.py
    python3 scripts/check_design_tokens.py --list   # findings, file by file
"""

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
UI = REPO_ROOT / "crates/apollia-desktop/ui"
APP_CSS = UI / "src/app.css"

TOKEN_DECL = re.compile(r"^\s*--([a-z0-9-]+)\s*:", re.M)
PALETTE = (
    "white|black|neutral|gray|grey|slate|zinc|stone|red|orange|amber|yellow|lime|green|"
    "emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia|pink|rose"
)
PROPS = (
    "bg|text|border|ring|from|to|via|fill|stroke|shadow|outline|divide|placeholder|"
    "accent|caret|decoration|ring-offset|border-[trblxy]"
)

RULES: list[tuple[str, re.Pattern[str]]] = [
    ("color", re.compile(r"(?<!&)#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})\b")),
    ("color", re.compile(r"\b(?:rgb|rgba|hsl|hsla)\(\s*\d")),
    ("color", re.compile(r"(?<![\w-])(?:[a-z-]+:)*(?:" + PROPS + r")-(?:" + PALETTE + r")(?:-\d{2,3})?(?:/\d+)?(?![\w-])")),
    ("color", re.compile(r"(?<![\w-])(?:[a-z-]+:)*(?:" + PROPS + r")-\[(?:#[0-9a-fA-F]{3,8}|rgba?\(|hsla?\(\s*\d)[^\]]*\]")),
    ("shadow", re.compile(r"\bbox-shadow\s*:(?!\s*(?:var\(|none|inherit))[^;]*\d")),
    ("shadow", re.compile(r"(?<![\w-])(?:[a-z-]+:)*shadow-\[[^\]]+\]")),
    ("radius", re.compile(r"\bborder(?:-(?:top|bottom)-(?:left|right))?-radius\s*:(?!\s*(?:var\(|calc\(\s*(?:var|TOKEN)|0(?![.\d])|50%|100%|9{2,}|inherit))[^;]*\d")),
    ("radius", re.compile(r"(?<![\w-])(?:[a-z-]+:)*rounded(?:-[trblse]{1,2})?-\[(?!9{2,}px|50%|100%)[^\]]+\]")),
    ("z-index", re.compile(r"\bz-index\s*:(?!\s*(?:var\(|calc\(\s*var|auto|-?1(?![.\d])|0(?![.\d])))\s*-?\d+")),
    ("z-index", re.compile(r"(?<![\w-])(?:[a-z-]+:)*z-(?:\[\d+\]|(?!0\b|10\b|auto)\d{2,})(?![\w-])")),
    ("motion", re.compile(r"\b(?:transition|animation)(?:-duration|-delay)?\s*:\s*[^;]*?\b\d+m?s\b")),
    ("motion", re.compile(r"\bcubic-bezier\(\s*[\d.]")),
    ("motion", re.compile(r"(?<![\w-])(?:[a-z-]+:)*duration-\d+(?![\w-])")),
    ("motion", re.compile(r"(?<![\w-])(?:[a-z-]+:)*duration-\[[^\]]+\]")),
    ("font-size", re.compile(r"(?<![\w-])(?:[a-z-]+:)*text-\[[\d.]+(?:px|rem)\]")),
    ("font-size", re.compile(r"\bfont-size\s*:(?!\s*(?:var\(|inherit))\s*[\d.]+(?:px|rem)")),
    ("size-px", re.compile(r"(?<![\w-])(?:[a-z-]+:)*(?:w|h|min-w|min-h|max-w|max-h|size|p[xytrbl]?|m[xytrbl]?|gap(?:-[xy])?|space-[xy]|inset(?:-[xy])?|top|left|right|bottom|leading|tracking|basis|translate-[xy])-\[[\d.]+px\]")),
]

FAMILIES = ("color", "shadow", "radius", "z-index", "motion", "font-size", "size-px")

COMMENT_LINE = re.compile(r"^\s*(?://|/\*|\*|<!--)")
ALLOWED_LITERALS = re.compile(r"\b(?:transparent|currentColor|inherit)\b")

MASKED = "\x00"

# ── The ratchet ──────────────────────────────────────────────────────────────
#
# Debt carried, file by file, with the count each file is allowed today. The
# list only shrinks: an entry whose file drops below its number fails until the
# number follows it down, and an entry whose file reaches zero leaves.
#
# `src/routes/` is absent on purpose. It was migrated to the tokens, and its
# absence is what keeps it migrated: a file with no entry is allowed nothing.
ALLOWED: dict[str, int] = {
    "src/app.css": 118,
    "src/components/agents/AgentActivity.svelte": 1,
    "src/components/agents/AgentDetail.svelte": 15,
    "src/components/agents/AgentLlmInfo.svelte": 1,
    "src/components/agents/AgentLogs.svelte": 13,
    "src/components/agents/AgentMessagesPanel.svelte": 5,
    "src/components/agents/AgentTriggers.svelte": 4,
    "src/components/agents/ApolliaChatConfigPanel.svelte": 13,
    "src/components/agents/InstallPackageDialog.svelte": 35,
    "src/components/automations/AutomationDefinitionForm.svelte": 1,
    "src/components/automations/AutomationWizard.svelte": 10,
    "src/components/chat/A2AWorkerBadge.svelte": 7,
    "src/components/chat/A2AWorkerSkillChip.svelte": 1,
    "src/components/chat/ActivityStrip.svelte": 3,
    "src/components/chat/AgentStatusCard.svelte": 4,
    "src/components/chat/AgentUnavailableBanner.svelte": 1,
    "src/components/chat/ApprovalCard.svelte": 13,
    "src/components/chat/ApprovalScopeSelect.svelte": 3,
    "src/components/chat/AskUserCard.svelte": 22,
    "src/components/chat/AskUserQuestion.svelte": 3,
    "src/components/chat/AskUserSummary.svelte": 3,
    "src/components/chat/AssertionInline.svelte": 9,
    "src/components/chat/AttachmentChip.svelte": 1,
    "src/components/chat/ChatConfigPanelBody.svelte": 17,
    "src/components/chat/ChatConversation.svelte": 13,
    "src/components/chat/ChatConversationHeader.svelte": 13,
    "src/components/chat/ChatInput.svelte": 8,
    "src/components/chat/ChatMessageBubble.svelte": 6,
    "src/components/chat/ChatPlanReview.svelte": 5,
    "src/components/chat/ChatPlanReviewBuilder.svelte": 5,
    "src/components/chat/CitationFootnote.svelte": 5,
    "src/components/chat/ContextIndicator.svelte": 4,
    "src/components/chat/EmptyAgentsState.svelte": 2,
    "src/components/chat/HitlFilesystemModal.svelte": 17,
    "src/components/chat/InputHints.svelte": 11,
    "src/components/chat/MentionResourceMenu.svelte": 5,
    "src/components/chat/MessageGroup.svelte": 2,
    "src/components/chat/MessageRenderer.svelte": 1,
    "src/components/chat/OperatorApprovalCard.svelte": 13,
    "src/components/chat/PerformanceHint.svelte": 2,
    "src/components/chat/PinnedResourceChip.svelte": 1,
    "src/components/chat/PlanModeChip.svelte": 1,
    "src/components/chat/QuickPicker.svelte": 4,
    "src/components/chat/ReasoningCard.svelte": 22,
    "src/components/chat/ReasoningCardShell.svelte": 3,
    "src/components/chat/ReasoningSequence.svelte": 2,
    "src/components/chat/RetryTimeline.svelte": 5,
    "src/components/chat/RichLinkPreview.svelte": 2,
    "src/components/chat/ScrollToBottomButton.svelte": 3,
    "src/components/chat/SessionNotFound.svelte": 1,
    "src/components/chat/ShortcutsHelpDialog.svelte": 1,
    "src/components/chat/SlashCommandMenu.svelte": 3,
    "src/components/chat/SourceCards.svelte": 7,
    "src/components/chat/StreamingCursor.svelte": 2,
    "src/components/chat/StreamingMessage.svelte": 5,
    "src/components/chat/StreamingText.svelte": 1,
    "src/components/chat/tool-bodies/BashBody.svelte": 1,
    "src/components/chat/tool-bodies/FileGlobBody.svelte": 1,
    "src/components/chat/tool-bodies/FileGrepBody.svelte": 1,
    "src/components/chat/tool-bodies/FileListBody.svelte": 1,
    "src/components/chat/tool-bodies/FileReadBody.svelte": 1,
    "src/components/chat/tool-bodies/FileWriteBody.svelte": 1,
    "src/components/chat/tool-bodies/HttpFetchBody.svelte": 1,
    "src/components/chat/tool-bodies/MemorySearchBody.svelte": 1,
    "src/components/chat/tool-bodies/PythonBody.svelte": 1,
    "src/components/chat/tool-bodies/TodoBody.svelte": 1,
    "src/components/chat/tool-bodies/WebReadBody.svelte": 1,
    "src/components/chat/tool-bodies/WebSearchBody.svelte": 1,
    "src/components/common/KeyboardHintOverlay.svelte": 3,
    "src/components/common/NextStepsPanel.svelte": 2,
    "src/components/companion/CompanionPanel.svelte": 4,
    "src/components/connections/catalogue/CatalogueSheet.svelte": 1,
    "src/components/inbox/ActivityRow.svelte": 2,
    "src/components/inbox/AskUserForm.svelte": 2,
    "src/components/inbox/RejectReasonDialog.svelte": 2,
    "src/components/integrations/ConnectorWizard.svelte": 1,
    "src/components/integrations/McpServerSettingsEditor.svelte": 39,
    "src/components/integrations/WizardStepAuth.svelte": 10,
    "src/components/integrations/WizardStepTest.svelte": 11,
    "src/components/llm/LlmStats.svelte": 1,
    "src/components/memory/InjectedMemorySheet.svelte": 9,
    "src/components/observability/ActiveHooksPanel.svelte": 1,
    "src/components/observability/AuditPurposeBanner.svelte": 1,
    "src/components/observability/AuditTrailTable.svelte": 1,
    "src/components/observability/ExecutionTrace.svelte": 5,
    "src/components/observability/LlmCostChart.svelte": 2,
    "src/components/observability/MailboxTable.svelte": 2,
    "src/components/observability/TimelineGlobal.svelte": 1,
    "src/components/onboarding/OnboardingAiSetup.svelte": 66,
    "src/components/onboarding/OnboardingChatStep.svelte": 9,
    "src/components/onboarding/OnboardingConfetti.svelte": 2,
    "src/components/onboarding/OnboardingModal.svelte": 10,
    "src/components/onboarding/OnboardingPermissionStep.svelte": 12,
    "src/components/onboarding/OnboardingProfileSelector.svelte": 8,
    "src/components/onboarding/OnboardingWelcome.svelte": 4,
    "src/components/project/ContextProvidersTab.svelte": 4,
    "src/components/project/ProviderCard.svelte": 6,
    "src/components/project/ProviderEditDialog.svelte": 12,
    "src/components/project/SnapshotPreview.svelte": 7,
    "src/components/settings/HotkeyCaptureDialog.svelte": 5,
    "src/components/settings/UnsavedChangesBadge.svelte": 2,
    "src/components/settings/about/AboutHero.svelte": 2,
    "src/components/settings/shortcuts/Keycap.svelte": 1,
    "src/components/settings/stt/SttTestCard.svelte": 1,
    "src/components/settings/tools/ToolRow.svelte": 1,
    "src/components/stt/RecordingOverlay.svelte": 3,
    "src/components/stt/TranscribingCard.svelte": 2,
    "src/components/stt/TranscriptWaveform.svelte": 1,
    "src/components/tasks/TasksDetailPanes.svelte": 6,
    "src/components/tasks/TasksFab.svelte": 1,
    "src/components/tasks/TasksListView.svelte": 4,
    "src/components/triggers/CreateTriggerDialog.svelte": 1,
    "src/components/triggers/TriggerLogs.svelte": 2,
    "src/lib/components/app/Sidebar.svelte": 6,
    "src/lib/components/app/Topbar.svelte": 1,
    "src/lib/components/feedback/KeyboardHint.svelte": 2,
    "src/lib/components/layout/EmptyState.svelte": 1,
    "src/lib/components/operator/ConversationRow.svelte": 15,
    "src/lib/components/operator/DetailHeader.svelte": 2,
    "src/lib/components/operator/EmptyState.svelte": 5,
    "src/lib/components/operator/EntityCard.svelte": 2,
    "src/lib/components/operator/ErrorBanner.svelte": 2,
    "src/lib/components/operator/FilterChipBar.svelte": 2,
    "src/lib/components/operator/HITLCard.svelte": 19,
    "src/lib/components/operator/InboxRow.svelte": 2,
    "src/lib/components/operator/Journal.svelte": 8,
    "src/lib/components/operator/ListPanel.svelte": 2,
    "src/lib/components/operator/NewProjectDialog.svelte": 31,
    "src/lib/components/operator/PageHeader.svelte": 5,
    "src/lib/components/operator/PlanDagPanel.svelte": 1,
    "src/lib/components/operator/PlanStepNode.svelte": 1,
    "src/lib/components/operator/PlanThinkingOverlay.svelte": 1,
    "src/lib/components/operator/ProjectCard.svelte": 12,
    "src/lib/components/operator/SectionTitle.svelte": 3,
    "src/lib/components/operator/SidebarHeader.svelte": 2,
    "src/lib/components/operator/SplitLayout.svelte": 4,
    "src/lib/components/operator/TaskRow.svelte": 12,
    "src/lib/components/operator/approval/ApprovalLevelSelector.svelte": 1,
    "src/lib/components/operator/approval/ApprovalTimer.svelte": 2,
    "src/lib/components/operator/badges/RiskBadge.svelte": 4,
    "src/lib/components/tour/TourStepCard.svelte": 1,
    "src/lib/components/ui/action-menu/ActionMenu.svelte": 1,
    "src/lib/components/ui/avatar/Avatar.svelte": 3,
    "src/lib/components/ui/badge/Badge.svelte": 14,
    "src/lib/components/ui/button/Button.svelte": 6,
    "src/lib/components/ui/checkbox/Checkbox.svelte": 2,
    "src/lib/components/ui/command/Command.svelte": 1,
    "src/lib/components/ui/command/CommandEmpty.svelte": 2,
    "src/lib/components/ui/command/CommandFooter.svelte": 1,
    "src/lib/components/ui/command/CommandGroup.svelte": 2,
    "src/lib/components/ui/command/CommandItem.svelte": 3,
    "src/lib/components/ui/command/Keycap.svelte": 2,
    "src/lib/components/ui/date-picker/DatePicker.svelte": 1,
    "src/lib/components/ui/date-picker/TimePicker.svelte": 1,
    "src/lib/components/ui/form-field/FormField.svelte": 3,
    "src/lib/components/ui/input/Input.svelte": 2,
    "src/lib/components/ui/markdown/markdown-prose.css": 4,
    "src/lib/components/ui/popover/Popover.svelte": 1,
    "src/lib/components/ui/progress/ProgressBar.svelte": 2,
    "src/lib/components/ui/progress/Spinner.svelte": 1,
    "src/lib/components/ui/radio/RadioItem.svelte": 1,
    "src/lib/components/ui/select/Select.svelte": 1,
    "src/lib/components/ui/separator/Separator.svelte": 2,
    "src/lib/components/ui/stepper/Stepper.svelte": 2,
    "src/lib/components/ui/tabs/TabBar.svelte": 1,
    "src/lib/components/ui/textarea/Textarea.svelte": 5,
    "src/lib/components/ui/toast/Toast.svelte": 1,
    "src/lib/components/ui/toggle/Toggle.svelte": 2,
}


def _tag_end(text: str, start: int) -> int:
    """Index of the `>` that closes the tag opened at `start`.

    Quotes and `{}` expressions are tracked, because Svelte attributes carry
    both: `onclick={() => run()}` holds a `>` that closes nothing.
    """
    i = start + 1
    quote = ""
    depth = 0
    while i < len(text):
        ch = text[i]
        if quote:
            if ch == "\\":
                i += 2
                continue
            if ch == quote:
                quote = ""
        elif ch in "\"'`":
            quote = ch
        elif ch == "{":
            depth += 1
        elif ch == "}":
            depth = max(0, depth - 1)
        elif ch == ">" and depth == 0:
            return i
        i += 1
    return len(text) - 1


def flaggable_mask(text: str, suffix: str) -> list[bool]:
    """Per-character map of where a literal could style something.

    In a `.css` file, everywhere. In a `.svelte` file: the `<script>` body (it
    is code), the `<style>` body (it is CSS), and the inside of every tag in
    the template (attributes). Element text content is prose and is left out,
    which is the single behaviour that separates this guard from the sweep it
    was promoted from.
    """
    n = len(text)
    if suffix == ".css":
        return [True] * n
    mask = [False] * n
    i = 0
    while i < n:
        if text.startswith("<!--", i):
            end = text.find("-->", i)
            i = n if end == -1 else end + 3
            continue
        for block in ("script", "style"):
            if text.startswith(f"<{block}", i) and (
                i + 1 + len(block) >= n or not text[i + 1 + len(block)].isalnum()
            ):
                body = _tag_end(text, i) + 1
                close = text.find(f"</{block}", body)
                end = n if close == -1 else close
                for k in range(body, end):
                    mask[k] = True
                i = end + 1
                break
        else:
            if text[i] == "<":
                end = _tag_end(text, i)
                for k in range(i, min(end + 1, n)):
                    mask[k] = True
                i = end + 1
            else:
                i += 1
            continue
    return mask


def strip_var_refs(line: str) -> str:
    """Blank out `var(--x)` and `hsl(var(--x) / .5)` so their digits do not trip a rule."""
    line = re.sub(r"hsla?\(\s*var\(--[a-z0-9-]+\)[^)]*\)", "TOKEN", line)
    return re.sub(r"var\(--[a-z0-9-]+(?:,[^)]*)?\)", "TOKEN", line)


def scan(text: str, suffix: str, is_app_css: bool) -> list[tuple[int, str, str]]:
    """Findings in one file, as (line number, family, literal)."""
    mask = flaggable_mask(text, suffix)
    findings: list[tuple[int, str, str]] = []
    in_block_comment = False
    in_token_decl = False
    offset = 0
    for n, raw in enumerate(text.splitlines(), 1):
        line_start, offset = offset, offset + len(raw) + 1
        if in_block_comment:
            if "*/" in raw or "-->" in raw:
                in_block_comment = False
            continue
        if COMMENT_LINE.match(raw):
            if (raw.lstrip().startswith("/*") and "*/" not in raw) or (
                raw.lstrip().startswith("<!--") and "-->" not in raw
            ):
                in_block_comment = True
            continue
        if in_token_decl:
            if ";" in raw:
                in_token_decl = False
            continue
        if TOKEN_DECL.match(raw) or (is_app_css and re.match(r"^\s*--", raw)):
            if ";" not in raw:
                in_token_decl = True
            continue
        masked = "".join(
            ch if mask[line_start + i] else MASKED for i, ch in enumerate(raw)
        )
        cleaned = strip_var_refs(masked)
        if suffix != ".css" and "http" not in cleaned:
            cleaned = re.sub(r"//.*$", "", cleaned)
        for family, rx in RULES:
            for m in rx.finditer(cleaned):
                literal = m.group(0)
                if ALLOWED_LITERALS.search(literal):
                    continue
                findings.append((n, family, literal.strip()))
    return findings


def tracked_files() -> list[Path]:
    # A pre-commit hook inherits GIT_DIR / GIT_WORK_TREE / GIT_INDEX_FILE
    # aimed at the repository root; kept, they would resolve `src` against the
    # wrong tree and this guard would report nothing measured on every commit.
    # `check_unimported_files.py` was already bitten by it.
    environment = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    out = subprocess.run(
        ["git", "ls-files", "src"],
        cwd=UI,
        capture_output=True,
        text=True,
        check=False,
        env=environment,
    )
    if out.returncode != 0:
        return []
    return [UI / f for f in out.stdout.split() if f.endswith((".svelte", ".css"))]


def measure(files: list[Path]) -> dict[str, list[tuple[int, str, str]]]:
    per_file: dict[str, list[tuple[int, str, str]]] = {}
    for path in files:
        text = path.read_text(encoding="utf-8", errors="replace")
        found = scan(text, path.suffix, path == APP_CSS)
        if found:
            per_file[str(path.relative_to(UI))] = found
    return per_file


def verdict(per_file: dict[str, list[tuple[int, str, str]]]) -> list[str]:
    """Ratchet failures, one line each. Empty when the tree matches its allowance."""
    failures: list[str] = []
    for rel, found in sorted(per_file.items()):
        allowed = ALLOWED.get(rel, 0)
        if len(found) > allowed:
            failures.append(
                f"{rel}: {len(found)} literal(s), {allowed} allowed. "
                f"Use a token, or raise nothing: this list only descends."
            )
        elif len(found) < allowed:
            failures.append(
                f"{rel}: {len(found)} literal(s) left, allowance still {allowed}. "
                f"Lower it to {len(found)} in scripts/check_design_tokens.py."
            )
    for rel in sorted(set(ALLOWED) - set(per_file)):
        failures.append(
            f"{rel}: allowance of {ALLOWED[rel]} but no literal left. "
            f"Drop the entry from scripts/check_design_tokens.py."
        )
    return failures


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--list", action="store_true", help="print every finding, file by file"
    )
    parser.add_argument(
        "--json", action="store_true", help="print the per-file counts as JSON"
    )
    args = parser.parse_args(argv)

    if not APP_CSS.exists():
        print("nothing measured: crates/apollia-desktop/ui/src/app.css is absent")
        return 2
    tokens = set(TOKEN_DECL.findall(APP_CSS.read_text(encoding="utf-8")))
    files = tracked_files()
    if not files or not tokens:
        print(
            f"nothing measured: {len(files)} file(s) listed by git, "
            f"{len(tokens)} token(s) read from app.css"
        )
        return 2

    per_file = measure(files)
    counts = {rel: len(found) for rel, found in per_file.items()}
    total = sum(counts.values())

    per_family: dict[str, int] = {}
    for found in per_file.values():
        for _, family, _ in found:
            per_family[family] = per_family.get(family, 0) + 1
    failures = verdict(per_file)

    if args.json:
        print(
            json.dumps(
                {
                    "files_scanned": len(files),
                    "tokens_declared": len(tokens),
                    "total": total,
                    "per_family": {f: per_family.get(f, 0) for f in FAMILIES},
                    "per_file": dict(sorted(counts.items())),
                    "allowance": sum(ALLOWED.values()),
                    "failures": failures,
                },
                indent=2,
            )
        )
        return 1 if failures else 0

    if args.list:
        for rel, found in sorted(per_file.items()):
            for n, family, literal in found:
                print(f"{rel}:{n}  {family:9}  {literal[:70]}")

    print(
        f"design tokens: {len(files)} file(s) scanned, {len(tokens)} token(s) "
        f"declared, {total} literal(s) in {len(per_file)} file(s)"
    )
    for family in FAMILIES:
        print(f"  {family:9} {per_family.get(family, 0):5d}")
    print(f"  allowance carried: {sum(ALLOWED.values())} in {len(ALLOWED)} file(s)")

    if failures:
        print(f"\n{len(failures)} file(s) off their allowance:")
        for line in failures:
            print(f"  {line}")
        return 1
    print("\nevery file is at or under its allowance")
    return 0


if __name__ == "__main__":
    sys.exit(main())
