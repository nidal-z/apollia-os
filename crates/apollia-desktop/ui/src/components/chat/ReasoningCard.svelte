<script lang="ts" module>
  import type { ReasoningItem as ReasoningItemKind } from "$lib/chat/reasoning";

  /**
   * The state a card opens in. What the user chose on this very block (the
   * stored "1" or "0") wins; then a caller override; then, for a reasoning
   * block only, the Appearance setting "unfold reasoning blocks by default";
   * then the per-kind default, where only a retry chain opens, being short and
   * a sign of trouble. A tool step always opens folded.
   */
  export function resolveInitialExpanded(input: {
    stored: string | null;
    defaultExpanded: boolean | undefined;
    expandByDefault: boolean;
    kind: ReasoningItemKind["kind"];
  }): boolean {
    if (input.stored === "1") return true;
    if (input.stored === "0") return false;
    if (input.defaultExpanded !== undefined) return input.defaultExpanded;
    if (input.kind === "retry") return true;
    const reasoning = input.kind === "thinking" || input.kind === "rationale";
    return reasoning && input.expandByDefault;
  }
</script>

<script lang="ts">
  /**
   * One item of an assistant turn, routed by kind: a reasoning fragment to
   * its block, a tool call to its step, a retry chain or a citation to the
   * shared card shell. It owns the folded or unfolded state of the item and
   * keeps it for the session.
   */

  import type { ReasoningItem } from "$lib/chat/reasoning";
  import ReasoningCardShell from "./ReasoningCardShell.svelte";
  import ReasoningBlock from "./ReasoningBlock.svelte";
  import ToolStep from "./ToolStep.svelte";
  import { get } from "svelte/store";
  import { reasoningExpandedByDefault } from "$lib/stores/chatAppearance";
  import { t } from "svelte-i18n";
  import { ExternalLink, RotateCcw, Quote } from "lucide-svelte";
  import { handleExternalLinkClick } from "$lib/utils/externalLink";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    item: ReasoningItem;
    /** `builder` is developer mode: a tool step adds its raw data. */
    skin?: "builder" | "operator";
    /** Optional override for the default expansion state. */
    defaultExpanded?: boolean;
    /** When false, expand state is not written to sessionStorage. */
    persist?: boolean;
    /** Opens the citation side panel - consumer wires the actual slide-over. */
    onCitation?: (item: Extract<ReasoningItem, { kind: "citation" }>) => void;
    /**
     * Chat session id, forwarded to the file bodies so a relative file path can
     * be revealed against the session working directory.
     */
    sessionId?: string;
    /** The answer the card belongs to, forwarded to the step. */
    messageId?: string;
  }

  let {
    item,
    skin = "operator",
    defaultExpanded,
    persist = true,
    onCitation,
    sessionId,
    messageId,
  }: Props = $props();

  const storageKey = $derived(`apollia.reasoning.expanded.${item.id}`);

  function loadExpanded(): boolean {
    const stored =
      persist && typeof sessionStorage !== "undefined"
        ? sessionStorage.getItem(storageKey)
        : null;
    return resolveInitialExpanded({
      stored,
      defaultExpanded,
      expandByDefault: get(reasoningExpandedByDefault),
      kind: item.kind,
    });
  }

  let expanded = $state(loadExpanded());

  function toggle() {
    expanded = !expanded;
    if (persist && typeof sessionStorage !== "undefined") {
      try {
        sessionStorage.setItem(storageKey, expanded ? "1" : "0");
      } catch {
        // quota or privacy mode - ignore
      }
    }
  }

  function hostname(url: string): string {
    try {
      return new URL(url).hostname.replace(/^www\./, "");
    } catch {
      return url;
    }
  }

  const testid = $derived(`reasoning-card-${item.kind}`);
</script>

{#if item.kind === "tool_call"}
  <ToolStep {item} {skin} {expanded} ontoggle={toggle} {sessionId} {messageId} />
{:else if item.kind === "thinking"}
  <ReasoningBlock
    content={item.content}
    kind="thinking"
    durationMs={item.duration_ms ?? null}
    {expanded}
    ontoggle={toggle}
  />
{:else if item.kind === "rationale"}
  <ReasoningBlock content={item.content} kind="rationale" {expanded} ontoggle={toggle} />
{:else if item.kind === "retry"}
  <ReasoningCardShell
    status={item.status}
    testid={testid}
    collapsible
    expanded={expanded}
    onToggle={toggle}
  >
    {#snippet icon()}<RotateCcw class="h-3 w-3 text-muted-foreground" />{/snippet}
    {#snippet title()}
      {$t("chat.reasoning.retry_label", {
        values: { n: item.attempts.length },
      })}
    {/snippet}
    {#snippet body()}
      <ol class="space-y-1">
        {#each item.attempts as a (a.index)}
          <li
            class="flex items-center gap-2 rounded px-2 py-1 text-caption {a.status === 'success'
              ? 'bg-success/5 text-success'
              : a.status === 'error' || a.status === 'rejected'
                ? 'bg-destructive/5 text-destructive'
                : 'bg-muted/30 text-muted-foreground'}"
          >
            <span class="font-mono text-micro tabular-nums opacity-70">#{a.index}</span>
            <span class="flex-1 truncate">{a.error ?? a.status}</span>
            {#if a.duration_ms != null}
              <span class="text-micro tabular-nums opacity-70">{a.duration_ms}ms</span>
            {/if}
          </li>
        {/each}
      </ol>
      {#if item.final_error}
        <p class="mt-1 text-caption text-destructive">{item.final_error}</p>
      {/if}
    {/snippet}
  </ReasoningCardShell>
{:else if item.kind === "citation"}
  <ReasoningCardShell status={item.status} testid={testid}>
    {#snippet icon()}<Quote class="h-3 w-3 text-muted-foreground" />{/snippet}
    {#snippet title()}
      <Button variant="ghost" size="sm"
        type="button"
        class="truncate text-left hover:underline"
        onclick={() => onCitation?.(item)}
      >{item.source}</Button>
    {/snippet}
    {#snippet body()}
      <p class="text-caption text-muted-foreground line-clamp-3">{item.excerpt}</p>
      {#if item.url}
        <a
          href={item.url}
          target="_blank"
          rel="noopener noreferrer"
          onclick={handleExternalLinkClick}
          class="mt-1 inline-flex items-center gap-1 text-micro text-muted-foreground hover:text-foreground"
        >
          <span class="font-mono">{hostname(item.url)}</span>
          <ExternalLink class="h-3 w-3" />
        </a>
      {/if}
    {/snippet}
  </ReasoningCardShell>
{/if}
