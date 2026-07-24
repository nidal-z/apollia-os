<!--
  StreamingMessage - the live assistant turn.

  Renders an append-only timeline that keeps reasoning captions AND tool-call
  rows on screen in true arrival order (reasoning fragment, tool, reasoning
  fragment, tool, ...) throughout the turn. Nothing is torn down when a tool
  call runs or a HITL approval card appears: the closed reasoning fragments are
  reconstructed from the token stream and interleaved with the tool events by
  the reasoning cursor captured at each tool's start. The currently-streaming
  (open) reasoning renders live below the timeline, and the answer text renders
  last, mirroring the finalized layout (header -> reasoning -> answer).
-->
<script lang="ts">
  import { t } from "svelte-i18n";
  import { fly } from "svelte/transition";
  import { Avatar } from "$lib/components/ui/avatar";
  import { Spinner } from "$lib/components/ui/progress";
  import { Check, X } from "lucide-svelte";
  import StreamingText from "./StreamingText.svelte";
  import ThinkingBadge from "./ThinkingBadge.svelte";
  import ReasoningCard from "./ReasoningCard.svelte";
  import ActivityStrip from "./ActivityStrip.svelte";
  import type { ReasoningItem } from "$lib/chat/reasoning";
  import { parseStream, isThinking as isActiveThinking } from "$lib/chat/streamParser";

  /** One tool invocation as reported by the runtime, tagged with the number of
   *  closed reasoning fragments already streamed when it started. */
  interface ToolStep {
    name: string;
    status: "running" | "done" | "refused";
    startedAt: number;
    durationMs?: number;
    reasoningCursor: number;
  }

  interface Props {
    /** Accumulated streamed text (per-session buffer snapshot). */
    text: string;
    /** Session mode - bubble only shown in "libre" mode. */
    sessionMode: "libre" | "agent";
    /** Assistant display name for the turn header. */
    agentName?: string | null;
    /** Ordered tool invocations for the current turn (append-only). */
    toolChain?: ToolStep[];
    /** `builder` shows raw reasoning; `operator` shows the semantic caption. */
    skin?: "builder" | "operator";
  }

  let {
    text,
    sessionMode,
    agentName = null,
    toolChain = [],
    skin = "builder",
  }: Props = $props();

  const displayName = $derived(agentName ?? $t("chat.assistant", { default: "Assistant" }));

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
  // Only non-thinking content goes into the answer bubble.
  const textContent = $derived(
    blocks
      .filter((b) => b.type !== "thinking")
      .map((b) => b.content)
      .join(""),
  );

  type TimelineEntry =
    | { kind: "reasoning"; id: string; content: string }
    | {
        kind: "tool";
        id: string;
        name: string;
        status: ToolStep["status"];
        durationMs?: number;
      };

  // Interleave closed reasoning fragments with tool rows: a tool started after
  // `reasoningCursor` fragments had closed, so every fragment up to that cursor
  // precedes it. Remaining fragments flush after the last tool.
  const timeline = $derived.by<TimelineEntry[]>(() => {
    const out: TimelineEntry[] = [];
    let ti = 0;
    toolChain.forEach((tool, idx) => {
      const upto = Math.min(tool.reasoningCursor, closedThinking.length);
      while (ti < upto) {
        out.push({ kind: "reasoning", id: `live-think-${ti}`, content: closedThinking[ti]! });
        ti += 1;
      }
      out.push({
        kind: "tool",
        id: `live-tool-${idx}-${tool.startedAt}`,
        name: tool.name,
        status: tool.status,
        durationMs: tool.durationMs,
      });
    });
    while (ti < closedThinking.length) {
      out.push({ kind: "reasoning", id: `live-think-${ti}`, content: closedThinking[ti]! });
      ti += 1;
    }
    return out;
  });

  function reasoningItem(id: string, content: string): ReasoningItem {
    return { id, kind: "thinking", status: "success", content };
  }

  // The activity strip is shown live (expanded) whenever the turn has produced
  // any reasoning or tool activity. When the turn finalizes and this component
  // is replaced by ChatMessageBubble, the collapsed strip is what remains.
  const hasActivity = $derived(timeline.length > 0 || activeThinking);
  const hasThinking = $derived(closedThinking.length > 0 || activeThinking);
</script>

<!-- Rendered in both libre and agent mode so an agent turn also shows its
     live reasoning and tools instead of a bare "thinking" placeholder while a
     slow local completion runs. -->
<div class="flex flex-col items-start" data-testid="chat-message-streaming" data-mode={sessionMode}>
  <div class="flex items-center gap-2 mb-1.5 px-0.5">
    <Avatar name={displayName} size="xs" ring={false} />
    <span class="text-[12px] font-medium text-muted-foreground">{displayName}</span>
  </div>

  <!-- Zone 1 (live): the activity strip stays expanded during the turn so the
       user can watch the reasoning captions + tool rows arrive in order, plus
       the open reasoning fragment streaming token by token. -->
  {#if hasActivity}
    <div class="w-full">
      <ActivityStrip open live {hasThinking} toolCount={toolChain.length}>
        {#if timeline.length > 0}
          <div class="space-y-1" data-testid="streaming-timeline">
            {#each timeline as entry (entry.id)}
              {#if entry.kind === "reasoning"}
                <ReasoningCard item={reasoningItem(entry.id, entry.content)} {skin} persist={false} />
              {:else}
                <div
                  class="flex items-center gap-1.5 py-0.5"
                  data-testid="streaming-tool-row"
                  in:fly={{ x: -8, duration: 200 }}
                >
                  <span class="flex-shrink-0">
                    {#if entry.status === "running"}
                      <Spinner size={11} class="text-primary/60" />
                    {:else if entry.status === "done"}
                      <Check size={11} class="text-success/70" />
                    {:else}
                      <X size={11} class="text-destructive/70" />
                    {/if}
                  </span>
                  <span class="truncate font-mono text-[11px] text-foreground/75">{entry.name}</span>
                  {#if entry.durationMs && entry.durationMs > 0}
                    <span class="ml-auto flex-shrink-0 text-[10px] text-muted-foreground/50">{entry.durationMs}ms</span>
                  {/if}
                </div>
              {/if}
            {/each}
          </div>
        {/if}

        <!-- Live (open) reasoning streams token by token as its own caption,
             matching the differentiated reasoning styling of the finalized
             captions. -->
        {#if activeThinking}
          <div
            class="mt-1 w-full border-l-2 border-primary/25 pl-2.5"
            data-testid="streaming-thinking-area"
          >
            <div class="flex flex-col gap-0.5">
              <ThinkingBadge />
              {#if activeThinkingContent.trim()}
                <span
                  class="block max-h-40 overflow-hidden whitespace-pre-wrap text-[12px] leading-snug text-foreground/70"
                  data-testid="streaming-active-thinking"
                >
                  {activeThinkingContent}
                </span>
              {/if}
            </div>
          </div>
        {/if}
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
