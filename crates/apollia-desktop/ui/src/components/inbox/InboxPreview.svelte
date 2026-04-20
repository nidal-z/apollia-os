<script lang="ts">
  import { t } from "svelte-i18n";
  import { Bot, MessageSquare } from "lucide-svelte";
  import InboxUrgencyBadge from "./InboxUrgencyBadge.svelte";
  import RiskBadge from "../hitl/RiskBadge.svelte";
  import PermissionPreview from "./preview/PermissionPreview.svelte";
  import PermissionImpact from "./preview/PermissionImpact.svelte";
  import ApprovalButtons from "../permissions/shared/ApprovalButtons.svelte";
  import { navigateTo } from "$lib/stores/navigation";
  import { emitAlwaysAcceptScope, type AlwaysAcceptScope } from "$lib/permissions/telemetry";
  import type { InboxItem } from "./types";
  import type { PreviewKind, PermissionImpactPayload } from "./preview/types";

  interface Props {
    item: InboxItem | null;
    submitting?: boolean;
    urgencyThresholdMs?: number;
    onaccept: (item: InboxItem) => void;
    onreject: (item: InboxItem) => void;
    onalwaysAccept: (item: InboxItem, scope?: AlwaysAcceptScope) => void;
  }

  let {
    item,
    submitting = false,
    urgencyThresholdMs,
    onaccept,
    onreject,
    onalwaysAccept,
  }: Props = $props();

  function previewKindFor(it: InboxItem): PreviewKind {
    if (it.kind === "filesystem") return "filesystem";
    if (it.kind === "bash") return "bash";
    if (it.kind === "tool" || it.kind === "always_accept") {
      const tool = it.toolName?.toLowerCase() ?? "";
      if (tool.startsWith("mcp:") || tool.includes("mcp")) return "mcp";
      if (tool.startsWith("http") || tool.includes("fetch") || tool.includes("request")) return "network";
      return "generic";
    }
    return "generic";
  }

  function impactFrom(it: InboxItem): PermissionImpactPayload {
    return {
      risk_level: it.risk?.level,
      risk_reasons: it.risk?.consequences,
      summary: it.risk?.impact ?? it.risk?.summary,
      consequence_if_approved: it.risk?.consequences?.[0],
      consequence_if_refused: it.risk?.rationale,
    };
  }

  function handleAlways(it: InboxItem, scope: AlwaysAcceptScope) {
    void emitAlwaysAcceptScope({
      scope,
      tool: it.toolName ?? "unknown",
      agent_kind: it.kind,
    });
    onalwaysAccept(it, scope);
  }
</script>

{#if !item}
  <div
    class="flex h-full flex-col items-center justify-center gap-2 text-muted-foreground"
    data-testid="inbox-preview-empty"
  >
    <Bot size={24} class="opacity-40" />
    <p class="text-sm">{$t("inbox.preview_empty")}</p>
  </div>
{:else}
  <article
    class="flex h-full flex-col shadow-[0_8px_24px_rgba(0,0,0,0.08)]"
    data-testid="inbox-preview"
    data-item-id={item.id}
  >
    <header class="flex items-start justify-between gap-3 border-b border-border px-5 py-4">
      <div class="flex min-w-0 flex-1 items-start gap-3">
        <div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-muted">
          {#if item.kind === "task"}
            <Bot size={16} />
          {:else}
            <MessageSquare size={16} />
          {/if}
        </div>
        <div class="min-w-0 flex-1">
          <h2 class="truncate text-base font-semibold">{item.agentName}</h2>
          <p class="truncate text-[12px] text-muted-foreground" title={item.summary}>
            {item.summary}
          </p>
          <div class="mt-1 flex items-center gap-2">
            {#if item.risk}
              <RiskBadge level={item.risk.level} />
            {/if}
            <InboxUrgencyBadge suspendedAt={item.suspendedAt} thresholdMs={urgencyThresholdMs} />
          </div>
        </div>
      </div>
    </header>

    <div class="flex-1 space-y-4 overflow-y-auto px-5 py-4 text-[13px]">
      {#if item.toolName}
        <section>
          <h3 class="mb-1 text-[11px] uppercase tracking-wide text-muted-foreground">{$t("inbox.preview.tool")}</h3>
          <code class="block rounded bg-muted px-2 py-1 text-[12px]">{item.toolName}</code>
        </section>
      {/if}

      <PermissionImpact payload={impactFrom(item)} />

      <section data-testid="inbox-preview-permission-preview">
        <h3 class="mb-1 text-[11px] uppercase tracking-wide text-muted-foreground">{$t("inbox.preview.impact")}</h3>
        <PermissionPreview
          toolCallId={item.id}
          kind={previewKindFor(item)}
          toolName={item.toolName ?? "unknown"}
        />
      </section>

      {#if item.risk?.thinking}
        <section>
          <h3 class="mb-1 text-[11px] uppercase tracking-wide text-muted-foreground">{$t("inbox.preview.thinking")}</h3>
          <p class="whitespace-pre-wrap text-muted-foreground">{item.risk.thinking}</p>
        </section>
      {/if}

      {#if item.risk?.rationale}
        <section>
          <h3 class="mb-1 text-[11px] uppercase tracking-wide text-muted-foreground">{$t("inbox.preview.rationale")}</h3>
          <p class="whitespace-pre-wrap">{item.risk.rationale}</p>
        </section>
      {/if}

      <section>
        <h3 class="mb-1 text-[11px] uppercase tracking-wide text-muted-foreground">{$t("inbox.preview.context")}</h3>
        {#if item.kind === "task"}
          <p class="whitespace-pre-wrap rounded glass-border glass-inset p-2">{item.source.prompt || $t("inbox.preview.no_context")}</p>
          {#if item.source.context}
            <details class="mt-2 text-[12px]">
              <summary class="cursor-pointer text-muted-foreground">{$t("inbox.preview.show_raw_context")}</summary>
              <pre class="mt-1 overflow-x-auto rounded bg-muted p-2 text-[11px]">{JSON.stringify(item.source.context, null, 2)}</pre>
            </details>
          {/if}
        {:else}
          <pre class="overflow-x-auto rounded bg-muted p-2 text-[11px]">{item.source.inputPreview}</pre>
        {/if}
      </section>

      <button
        type="button"
        class="inline-flex items-center gap-1 text-[11px] text-primary hover:underline"
        onclick={() => navigateTo("settings-permission-rules")}
        data-testid="inbox-preview-manage-rules"
      >
        {$t("permissions.rules.manage_link")} →
      </button>
    </div>

    <footer class="sticky bottom-0 flex flex-col gap-2 border-t border-border bg-card/95 px-5 py-3 shadow-[0_-4px_16px_rgba(0,0,0,0.05)] backdrop-blur">
      <ApprovalButtons
        {submitting}
        onaccept={() => onaccept(item)}
        onrefuse={() => onreject(item)}
        onalways={(scope) => handleAlways(item, scope)}
      />
    </footer>
  </article>
{/if}
