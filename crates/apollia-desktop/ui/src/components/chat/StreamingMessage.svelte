<!--
  StreamingMessage - the live assistant turn.

  Two zones, mirroring the finalized layout: a quiet reasoning strip (thoughts
  as flat narrated captions, shown open while streaming) and, below it, the tool
  calls as visible rows in the thread flow. Nothing is torn down when a tool
  runs or a HITL approval appears. The open (still-streaming) reasoning fragment
  renders live at the end of the strip, and the answer text renders last.
-->
<script lang="ts">
  import { t, locale } from "svelte-i18n";
  import { fly } from "svelte/transition";
  import { Avatar } from "$lib/components/ui/avatar";
  import { Spinner } from "$lib/components/ui/progress";
  import { Check, X } from "lucide-svelte";
  import StreamingText from "./StreamingText.svelte";
  import ActivityStrip from "./ActivityStrip.svelte";
  import { formatDurationSeconds } from "$lib/chat/duration";
  import { parseStream, isThinking as isActiveThinking } from "$lib/chat/streamParser";
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

  // Live rows for the tool chain, in arrival order. These render as visible
  // rows in the thread flow (below the reasoning strip), mirroring the two-zone
  // finalized layout where the tool calls sit outside the collapsed strip.
  const toolRows = $derived(
    toolChain.map((tool, idx) => ({
      id: `live-tool-${idx}-${tool.startedAt}`,
      name: tool.name,
      status: tool.status,
      durationMs: tool.durationMs,
    })),
  );

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

  <!-- Zone 1 (live): the reasoning strip stays expanded during the turn so the
       user can watch the thoughts arrive as flat narrated captions, plus the
       open fragment streaming token by token. -->
  {#if hasThinking}
    <div class="w-full">
      <ActivityStrip open live>
        <div class="flex flex-col gap-1" data-testid="streaming-timeline">
          {#each closedThinking as frag, i (`live-think-${i}`)}
            <div
              class="min-w-0 whitespace-pre-wrap text-[12.5px] italic leading-relaxed text-muted-foreground"
              in:fly={{ x: -8, duration: 200 }}
            >{frag}</div>
          {/each}

          <!-- Live (open) reasoning streams token by token as its own caption,
               matching the finalized flat caption styling. -->
          {#if activeThinking && activeThinkingContent.trim()}
            <div
              class="min-w-0 max-h-40 overflow-hidden whitespace-pre-wrap text-[12.5px] italic leading-relaxed text-muted-foreground"
              data-testid="streaming-active-thinking"
            >{activeThinkingContent}</div>
          {/if}
        </div>
      </ActivityStrip>
    </div>
  {/if}

  <!-- Zone 2 (live): tool rows appear visible in the flow as they run, below
       the reasoning strip, mirroring the finalized two-zone layout. -->
  {#if toolRows.length > 0}
    <div class="w-full space-y-0.5" data-testid="streaming-tools">
      {#each toolRows as row (row.id)}
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
      {/each}
    </div>
  {/if}

  <!-- Main response (thinking-free), transparent in the flat thread. -->
  {#if textContent || !activeThinking}
    <div class="w-full py-1 text-[14px] text-foreground">
      <StreamingText text={textContent} />
    </div>
  {/if}
</div>
