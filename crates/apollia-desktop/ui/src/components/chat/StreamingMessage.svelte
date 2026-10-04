<!--
  StreamingMessage - the live assistant turn.

  Mirrors the finalized layout: one timeline in the order things happen
  (thought, action, thought, action), then the answer. A closed thought folds
  into its block, titled by the time it took, measured here as it streams; the
  one still streaming stays open at the end of the timeline so the user can
  watch it arrive. Nothing is torn down when a tool runs or an approval
  appears.
-->
<script lang="ts">
  import { t, locale } from "svelte-i18n";
  import { fly } from "svelte/transition";
  import { Avatar } from "$lib/components/ui/avatar";
  import { Spinner } from "$lib/components/ui/progress";
  import { Check, X } from "lucide-svelte";
  import StreamingText from "./StreamingText.svelte";
  import ReasoningCard from "./ReasoningCard.svelte";
  import ReasoningBlock from "./ReasoningBlock.svelte";
  import { createReasoningClock } from "$lib/chat/reasoningTiming";
  import { stepActionKey, stepIconByName } from "$lib/chat/toolStep";
  import { buildLiveSequence } from "$lib/chat/reasoning";
  import type { LiveRow, LiveToolStep, ReasoningItem } from "$lib/chat/reasoning";
  import { formatSecondsLocalized } from "$lib/chat/duration";
  import { parseStream, isThinking as isActiveThinking, answerText } from "$lib/chat/streamParser";
  import { resolveToolDisplay, humanizeToolName } from "$lib/tools/tool-display";

  // Clear, human name for a tool's live row, the same action name the finished
  // step carries ("Recherche sur le web" rather than the raw "web_search").
  function toolLabel(name: string): string {
    const key = stepActionKey(name);
    if (key) return $t(key);
    const d = resolveToolDisplay({
      tool_name: name,
      input: {},
      output: null,
      status: "executed",
      duration_ms: null,
    });
    return $t(d.labelKey, {
      values: d.templateParams,
      default: humanizeToolName(name),
    });
  }

  interface Props {
    /** Accumulated streamed text (per-session buffer snapshot). */
    text: string;
    /** Session mode - bubble only shown in "libre" mode. */
    sessionMode: "libre" | "agent";
    /** Assistant display name for the turn header. */
    agentName?: string | null;
    /** Ordered tool invocations for the current turn (append-only). */
    toolChain?: LiveToolStep[];
    /**
     * Accepted for API parity with the finalized turn, but unused live: the
     * streaming reasoning captions and compact tool rows carry no builder/
     * operator split (no per-tool body is rendered until the turn finalizes).
     */
    skin?: "builder" | "operator";
  }

  let {
    text,
    sessionMode,
    agentName = null,
    toolChain = [],
  }: Props = $props();

  const displayName = $derived(agentName ?? $t("chat.assistant"));

  const blocks = $derived(parseStream(text));
  const activeThinking = $derived(isActiveThinking(blocks));
  // Closed reasoning fragments, in stream order.
  const closedThinking = $derived(
    blocks
      .filter((b) => b.type === "thinking" && b.closed)
      .map((b) => b.content.trim())
      .filter((s) => s.length > 0),
  );
  // Live reasoning text for the open thinking block, streamed token by token.
  const activeThinkingContent = $derived(
    activeThinking ? (blocks.at(-1)?.content ?? "") : "",
  );
  // Only non-thinking content goes into the answer bubble. `answerText` also
  // strips the stream markers, which is what lets `StreamingText` render the
  // result as plain markdown without parsing it a second time.
  const textContent = $derived(answerText(blocks));

  // The live timeline: closed thoughts and tool calls interleaved in the order
  // they happened, built by the same rule the finalized turn uses.
  const timeline = $derived(buildLiveSequence(closedThinking, toolChain));

  // Each thought is timed from its first token to its close. The measure is
  // also remembered by its text, for the finished turn to title its block.
  const clock = createReasoningClock();
  let measured = $state<Record<string, number>>({});
  $effect(() => {
    const fragments = blocks
      .filter((b) => b.type === "thinking")
      .map((b) => ({ content: b.content, closed: b.closed }));
    const durations = clock.observe(fragments);
    fragments.forEach((fragment, i) => {
      const ms = durations[i];
      const key = fragment.content.trim();
      if (ms != null && key && measured[key] === undefined) {
        measured = { ...measured, [key]: ms };
      }
    });
  });

  /** A closed thought, shaped for the same folded block a finalized turn uses. */
  function thoughtItem(row: Extract<LiveRow, { kind: "thought" }>): ReasoningItem {
    return {
      id: row.id,
      kind: "thinking",
      status: "success",
      content: row.content,
      duration_ms: measured[row.content.trim()] ?? null,
    };
  }

  const hasThinking = $derived(closedThinking.length > 0 || activeThinking);
</script>

<!-- Rendered in both libre and agent mode so an agent turn also shows its
     live reasoning and tools instead of a bare "thinking" placeholder while a
     slow local completion runs. -->
<div class="flex flex-col items-start" data-testid="chat-message-streaming" data-mode={sessionMode}>
  <div class="flex items-center gap-2 mb-3 px-0.5">
    <Avatar name={displayName} size="xs" ring={false} />
    <span class="text-body-sm font-semibold text-foreground">{displayName}</span>
  </div>

  <!-- Live: the same timeline as a finalized turn, growing as the work
       happens. -->
  {#if hasThinking || timeline.length > 0}
    <div class="mb-1 flex w-full flex-col gap-0.5" data-testid="streaming-timeline">
      {#each timeline as row (row.id)}
        {#if row.kind === "thought"}
          <div in:fly={{ x: -8, duration: 200 }}>
            <ReasoningCard item={thoughtItem(row)} persist={false} />
          </div>
        {:else}
          {@const RowIcon = stepIconByName(row.name)}
          <div
            class="flex min-w-0 items-center gap-2 py-1 pl-5 text-body-sm"
            data-testid="streaming-tool-row"
            in:fly={{ x: -8, duration: 200 }}
          >
            <span class="flex-none text-muted-foreground" aria-hidden="true"><RowIcon class="h-3.5 w-3.5" /></span>
            <span class="flex-none font-medium text-foreground">{toolLabel(row.name)}</span>
            <span class="ml-auto flex flex-none items-center gap-1.5 text-caption tabular-nums text-muted-foreground">
              {#if row.status === "running"}
                <Spinner size={13} class="text-primary" label={$t("chat.step.running")} />
              {:else if row.status === "done"}
                <Check size={14} class="text-success" />
              {:else}
                <X size={14} class="text-destructive" />
              {/if}
              {#if row.durationMs && row.durationMs > 0}
                <span>{formatSecondsLocalized(row.durationMs, $locale ?? "en")} s</span>
              {/if}
            </span>
          </div>
        {/if}
      {/each}

      <!-- The open thought streams token by token, in place, at the end of the
           timeline. It is the one reasoning block shown open: it is happening
           now. -->
      {#if activeThinking && activeThinkingContent.trim()}
        <div data-testid="streaming-active-thinking">
          <ReasoningBlock content={activeThinkingContent} live expanded ontoggle={() => {}} />
        </div>
      {/if}
    </div>
  {/if}

  <!-- Main response (thinking-free), transparent in the flat thread. -->
  {#if textContent || !activeThinking}
    <div class="w-full py-1 text-body-md text-foreground">
      <StreamingText text={textContent} />
    </div>
  {/if}
</div>
