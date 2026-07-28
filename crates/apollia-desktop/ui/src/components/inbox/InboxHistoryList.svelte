<script lang="ts">
  /**
   * Recently resolved HITL decisions (accept / always-accept / refuse), with
   * the refusal reason surfaced. A partial history failure renders a humanized
   * error box here without wiping the main pending list.
   */
  import { t } from "svelte-i18n";
  import { CheckCircle2, XCircle, ShieldCheck } from "lucide-svelte";
  import { SectionTitle } from "$lib/components/operator";
  import InboxErrorBox from "./InboxErrorBox.svelte";
  import type { ResolvedChatApproval } from "$lib/types";

  interface Props {
    history: ResolvedChatApproval[];
    error: unknown;
    relTime: (iso: string) => string;
    onRetry: () => void;
  }

  let { history, error, relTime, onRetry }: Props = $props();
</script>

<SectionTitle count={history.length}>{$t("inbox.history_title")}</SectionTitle>
<div class="px-8 pb-10">
  {#if error}
    <InboxErrorBox {error} {onRetry} testid="inbox-history-error" />
  {:else}
    <ul class="divide-y divide-border/60 rounded-xl border border-border/60 bg-card">
      {#each history as h (h.message_id + "::" + h.tool_name + "::" + h.resolved_at)}
        {@const isAccept = h.decision === "accept"}
        {@const isAlways = h.decision === "always_accept"}
        <li class="flex items-start gap-3 px-4 py-2.5 text-body-xs">
          <span class="mt-0.5 shrink-0" aria-hidden="true">
            {#if isAccept}
              <CheckCircle2 size={14} class="text-success" />
            {:else if isAlways}
              <ShieldCheck size={14} class="text-primary" />
            {:else}
              <XCircle size={14} class="text-destructive" />
            {/if}
          </span>
          <div class="min-w-0 flex-1">
            <div class="flex items-baseline justify-between gap-2">
              <div class="flex min-w-0 items-baseline gap-2">
                <code class="truncate font-mono text-caption text-foreground">{h.tool_name}</code>
                <span class="text-caption text-muted-foreground">
                  {#if isAccept}{$t("inbox.history.accepted")}
                  {:else if isAlways}{$t("inbox.history.always_accepted")}
                  {:else}{$t("inbox.history.refused")}{/if}
                </span>
              </div>
              <span class="shrink-0 font-mono text-caption text-muted-foreground/70" title={h.resolved_at}>
                {relTime(h.resolved_at)}
              </span>
            </div>
            {#if !isAccept && !isAlways && h.reason}
              <p class="mt-0.5 line-clamp-2 text-caption text-destructive/80" title={h.reason}>
                <span class="font-medium">{$t("inbox.history.reason")}</span> {h.reason}
              </p>
            {/if}
            <p class="mt-0.5 text-caption text-muted-foreground/60">
              {$t("inbox.history.session")} <code class="font-mono">{h.session_id.slice(0, 8)}</code>
            </p>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</div>
