<!--
  StreamingMessage - the live assistant turn.

  Mirrors the finalized layout: a quiet summary line, then one timeline in the
  order things happen (thought, action, thought, action), then the answer.
  Closed thoughts collapse into their row like they do in a finalized turn; the
  one still streaming stays visible at the end of the timeline so the user can
  watch it arrive. Nothing is torn down when a tool runs or a HITL approval
  appears.
-->
<script lang="ts">
  import { t, locale } from "svelte-i18n";
  import { fly } from "svelte/transition";
  import { Avatar } from "$lib/components/ui/avatar";
  import { Spinner } from "$lib/components/ui/progress";
  import { Check, X } from "lucide-svelte";
  import StreamingText from "./StreamingText.svelte";
  import ActivityStrip from "./ActivityStrip.svelte";
  import ReasoningCard from "./ReasoningCard.svelte";
  import { buildLiveSequence } from "$lib/chat/reasoning";
  import type { LiveRow, LiveToolStep, ReasoningItem } from "$lib/chat/reasoning";
  import { formatDurationSeconds } from "$lib/chat/duration";
  import { parseStream, isThinking as isActiveThinking, answerText } from "$lib/chat/streamParser";
  import { resolveToolDisplay, humanizeToolName } from "$lib/tools/tool-display";

  // Clear, human name for a tool's live row (e.g. "Recherche web" rather than
  // the raw "web_search", or "Google Calendar" rather than "gcal.list_events").
  // The raw technical name stays available as the row's title tooltip and in the
  // finalized, expandable trace for builders.
  function toolLabel(name: string): string {
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

  /** A closed thought, shaped for the same collapsed row a finalized turn uses. */
  function thoughtItem(row: Extract<LiveRow, { kind: "thought" }>): ReasoningItem {
    return {
      id: row.id,
      kind: "thinking",
      status: "success",
      content: row.content,
    };
  }

  // The reasoning strip is shown live (expanded) whenever the turn has produced
  // any thinking. When the turn finalizes and this component is replaced by
  // ChatMessageBubble, the collapsed strip is what remains.
  const hasThinking = $derived(closedThinking.length > 0 || activeThinking);
</script>

<!-- Rendered in both libre and agent mode so an agent turn also shows its
     live reasoning and tools instead of a bare "thinking" placeholder while a
     slow local completion runs. -->
<div class="flex flex-col items-start" data-testid="chat-message-streaming" data-mode={sessionMode}>
  <div class="flex items-center gap-2 mb-3 px-0.5">
    <Avatar name={displayName} size="xs" ring={false} />
    <span class="text-[13px] font-semibold text-foreground">{displayName}</span>
  </div>

  <!-- Live: the same activity block as a finalized turn, held open so the user
       can watch the work happen. It collapses on its own once the turn is
       replaced by ChatMessageBubble. -->
  {#if hasThinking || timeline.length > 0}
    <div class="w-full">
      <ActivityStrip open live>
        <div data-testid="streaming-timeline">
          {#each timeline as row (row.id)}
            {#if row.kind === "thought"}
              <div in:fly={{ x: -8, duration: 200 }}>
                <ReasoningCard item={thoughtItem(row)} persist={false} />
              </div>
            {:else}
              <div
                class="flex items-center gap-1.5 py-0.5"
                data-testid="streaming-tool-row"
                in:fly={{ x: -8, duration: 200 }}
              >
                <span class="flex-shrink-0">
                  {#if row.status === "running"}
                    <Spinner size={11} class="text-primary/60" />
                  {:else if row.status === "done"}
                    <Check size={11} class="text-success/70" />
                  {:else}
                    <X size={11} class="text-destructive/70" />
                  {/if}
                </span>
                <span
                  class="truncate text-[11.5px] text-foreground/80"
                  title={row.name}
                >{toolLabel(row.name)}</span>
                {#if row.durationMs && row.durationMs > 0}
                  <span class="ml-auto flex-shrink-0 text-[10px] text-muted-foreground/50"
                    >{formatDurationSeconds(row.durationMs, $locale ?? "en")} s</span
                  >
                {/if}
              </div>
            {/if}
          {/each}

          <!-- The open thought streams token by token, in place, at the end of
               the timeline. It is the one piece of reasoning shown expanded:
               it is happening now. -->
          {#if activeThinking && activeThinkingContent.trim()}
            <div
              class="min-w-0 max-h-40 overflow-hidden whitespace-pre-wrap py-1 text-[12.5px] italic leading-relaxed text-muted-foreground"
              data-testid="streaming-active-thinking"
            >{activeThinkingContent}</div>
          {/if}
        </div>
      </ActivityStrip>
    </div>
  {/if}

  <!-- Main response (thinking-free), transparent in the flat thread. -->
  {#if textContent || !activeThinking}
    <div class="w-full py-1 text-[14px] text-foreground">
      <StreamingText text={textContent} />
    </div>
  {/if}
</div>
