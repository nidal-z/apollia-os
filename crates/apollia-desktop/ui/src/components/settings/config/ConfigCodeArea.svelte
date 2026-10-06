<script module lang="ts">
  /** The marks a line of the editor can carry, at most one of each. */
  export interface LineMarks {
    /** The first fault of the line, as the runtime words it. */
    error?: string;
    locked?: boolean;
    restart?: boolean;
    modified?: boolean;
  }

  /** The tone of a line, the strongest mark first. */
  export function lineTone(marks: LineMarks | undefined): "error" | "locked" | "restart" | "none" {
    if (!marks) return "none";
    if (marks.error !== undefined) return "error";
    if (marks.locked) return "locked";
    if (marks.restart) return "restart";
    return "none";
  }
</script>

<script lang="ts">
  /**
   * The text of `apollia.toml` as an editable code area: a gutter of line
   * numbers and marks, a layer colored by Shiki, and over it a transparent
   * textarea that takes the typing. The three share one font and one line
   * height, and scroll together inside one container.
   */
  import { untrack } from "svelte";
  import { t } from "svelte-i18n";
  import { AlertCircle, Clock, Lock } from "lucide-svelte";
  import type { CodeToken } from "$lib/utils/shikiHydrator";
  import { caretPosition, lineOffset, splitLines } from "$lib/settings/configText";

  interface Props {
    text: string;
    /** The colored lines, or null while the colors are not loaded. */
    tokens: CodeToken[][] | null;
    marks: Map<number, LineMarks>;
    /** Takes the new text; false undoes the keystroke (a locked line). */
    onedit: (text: string) => boolean;
    oncaret?: (position: { line: number; column: number }) => void;
    /** A line to place the caret on and show, consumed by the caller. */
    focusLine?: number | null;
  }

  let { text, tokens, marks, onedit, oncaret, focusLine = null }: Props = $props();

  let area = $state<HTMLTextAreaElement | null>(null);
  let scroller = $state<HTMLDivElement | null>(null);
  let before: { value: string; start: number; end: number } | null = null;

  const lines = $derived(splitLines(text));
  const longest = $derived(lines.reduce((max, line) => Math.max(max, line.length), 0));
  const colored = $derived(tokens !== null && tokens.length === lines.length ? tokens : null);

  // The typed value is pushed back only when it differs, so the caret of a
  // keystroke stays where it is; an outside change (reload, defaults) lands.
  $effect(() => {
    if (area && area.value !== text) area.value = text;
  });

  // Placed once per requested line: the text is read untracked, so typing
  // does not send the caret back to it.
  $effect(() => {
    if (!area || focusLine === null) return;
    const offset = lineOffset(untrack(() => text), focusLine);
    area.focus();
    area.setSelectionRange(offset, offset);
    reportCaret();
    const row = scroller?.querySelector<HTMLElement>(`[data-line="${focusLine}"]`);
    row?.scrollIntoView({ block: "center" });
  });

  function remember(): void {
    if (!area) return;
    before = { value: area.value, start: area.selectionStart, end: area.selectionEnd };
  }

  function onInput(): void {
    if (!area) return;
    if (onedit(area.value)) {
      reportCaret();
      return;
    }
    if (before) {
      area.value = before.value;
      area.setSelectionRange(before.start, before.end);
    }
  }

  function reportCaret(): void {
    if (area) oncaret?.(caretPosition(area.value, area.selectionStart));
  }

  function onKeydown(e: KeyboardEvent): void {
    if (e.key !== "Tab" || e.metaKey || e.ctrlKey || e.altKey || !area) return;
    e.preventDefault();
    remember();
    area.setRangeText("  ", area.selectionStart, area.selectionEnd, "end");
    onInput();
  }

  function gutterTitle(mark: LineMarks | undefined): string | undefined {
    if (!mark) return undefined;
    if (mark.error !== undefined) return $t("settings.config_editor.mark_error", { values: { message: mark.error } });
    if (mark.locked) return $t("settings.policy_locked");
    if (mark.restart) return $t("settings.config_editor.mark_restart");
    if (mark.modified) return $t("settings.config_editor.mark_modified");
    return undefined;
  }

  const TONE_ROW: Record<ReturnType<typeof lineTone>, string> = {
    error: "bg-destructive/10",
    locked: "bg-muted/60",
    restart: "bg-warning/10",
    none: "",
  };
</script>

<div
  bind:this={scroller}
  class="relative max-h-[60vh] min-h-48 overflow-auto rounded-lg border border-border bg-surface-1"
  data-testid="config-editor-code"
>
  <div class="flex min-w-full w-max">
    <!-- Gutter: line numbers and the marks of each line. -->
    <div
      class="sticky left-0 z-sticky flex-none select-none border-r border-border bg-surface-2 py-2 font-mono text-code-sm leading-6 text-muted-foreground"
      aria-hidden="true"
    >
      {#each lines as _, index (index)}
        {@const line = index + 1}
        {@const mark = marks.get(line)}
        <div
          class="flex h-6 items-center gap-1 pl-1 pr-2"
          data-testid="config-editor-gutter-{line}"
          data-tone={lineTone(mark)}
          data-modified={mark?.modified ? "true" : undefined}
          title={gutterTitle(mark)}
        >
          <span
            class="h-4 w-0.5 flex-none rounded-full {mark?.modified ? 'bg-primary' : 'bg-transparent'}"
          ></span>
          <span class="flex w-3.5 flex-none justify-center">
            {#if mark?.error !== undefined}
              <AlertCircle size={12} class="text-destructive" />
            {:else if mark?.locked}
              <Lock size={12} />
            {:else if mark?.restart}
              <Clock size={12} class="text-warning-a11y" />
            {/if}
          </span>
          <span class="min-w-6 text-right tabular-nums">{line}</span>
        </div>
      {/each}
    </div>

    <!-- Code: the colored layer, the line marks behind it, the textarea over it. -->
    <div class="relative flex-1 py-2">
      <div class="pointer-events-none font-mono text-code-sm leading-6" aria-hidden="true">
        {#each lines as raw, index (index)}
          {@const line = index + 1}
          {@const mark = marks.get(line)}
          <div
            class="flex h-6 whitespace-pre px-3 {TONE_ROW[lineTone(mark)]}"
            data-line={line}
            data-testid="config-editor-line-{line}"
            data-tone={lineTone(mark)}
          >
            <span class="text-foreground">
              {#if colored}
                {#each colored[index]! as token, k (k)}<span style={token.color ? `color: ${token.color}` : undefined}>{token.content}</span>{/each}
              {:else}{raw}{/if}
            </span>
            {#if mark?.error !== undefined}
              <span
                class="ml-4 truncate font-sans text-caption italic text-destructive"
                data-testid="config-editor-error-{line}">{mark.error}</span
              >
            {:else if mark?.locked}
              <span
                class="ml-4 inline-flex items-center gap-1 font-sans text-caption text-muted-foreground"
                data-testid="config-editor-locked-{line}">{$t("settings.policy_locked")}</span
              >
            {:else if mark?.restart}
              <span
                class="ml-4 font-sans text-caption text-warning-a11y"
                data-testid="config-editor-restart-{line}"
                >{$t("settings.config_editor.mark_restart")}</span
              >
            {/if}
          </div>
        {/each}
      </div>
      <textarea
        bind:this={area}
        class="absolute inset-0 m-0 block h-full w-full resize-none overflow-hidden whitespace-pre border-0 bg-transparent px-3 py-2 font-mono text-code-sm leading-6 text-transparent caret-foreground outline-none selection:bg-primary/20"
        style="min-width: calc({longest}ch + 3rem);"
        wrap="off"
        spellcheck="false"
        autocapitalize="off"
        autocomplete="off"
        aria-label={$t("settings.config_editor.aria_label")}
        data-testid="config-editor-textarea"
        onbeforeinput={remember}
        oninput={onInput}
        onkeydown={onKeydown}
        onkeyup={reportCaret}
        onclick={reportCaret}
        onselect={reportCaret}
      ></textarea>
    </div>
  </div>
</div>
