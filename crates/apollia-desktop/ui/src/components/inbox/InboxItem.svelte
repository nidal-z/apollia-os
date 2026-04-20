<script lang="ts">
  import { t } from "svelte-i18n";
  import { Bot, MessageSquare, Terminal, FolderOpen, ShieldCheck } from "lucide-svelte";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import InboxUrgencyBadge from "./InboxUrgencyBadge.svelte";
  import RiskBadge from "../hitl/RiskBadge.svelte";
  import type { InboxItem } from "./types";

  interface Props {
    item: InboxItem;
    selected: boolean;
    active: boolean;
    urgencyThresholdMs?: number;
    onselect: () => void;
    ontoggle: () => void;
  }

  let { item, selected, active, urgencyThresholdMs, onselect, ontoggle }: Props = $props();

  const kindMeta = $derived.by(() => {
    switch (item.kind) {
      case "task":
        return { icon: Bot, label: $t("inbox.kind.task"), tone: "text-primary" };
      case "tool":
        return { icon: MessageSquare, label: $t("inbox.kind.tool"), tone: "text-secondary" };
      case "bash":
        return { icon: Terminal, label: $t("inbox.kind.bash"), tone: "text-warning" };
      case "filesystem":
        return { icon: FolderOpen, label: $t("inbox.kind.filesystem"), tone: "text-info" };
      case "always_accept":
        return { icon: ShieldCheck, label: $t("inbox.kind.always_accept"), tone: "text-muted-foreground" };
    }
  });

  function handleRowClick(event: MouseEvent) {
    // Let checkbox clicks bubble to their own handler.
    if ((event.target as HTMLElement).closest("[data-inbox-checkbox]")) return;
    onselect();
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      onselect();
    }
  }
</script>

<div
  role="button"
  aria-pressed={active}
  tabindex="0"
  onclick={handleRowClick}
  onkeydown={handleKeydown}
  class="group flex cursor-pointer gap-3 rounded-md border border-border/60 bg-card p-3 transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-ring/40 {active
    ? 'border-primary bg-primary/5 shadow-[0_4px_12px_rgba(0,0,0,0.06)]'
    : 'hover:bg-muted/40'}"
  data-testid="inbox-item"
  data-kind={item.kind}
  data-item-id={item.id}
>
  <div data-inbox-checkbox class="pt-0.5">
    <Checkbox
      checked={selected}
      onchange={() => ontoggle()}
      data-testid="inbox-item-checkbox"
    />
  </div>

  <div class="flex min-w-0 flex-1 flex-col gap-1">
    <div class="flex items-center gap-2">
      <div class="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-muted {kindMeta.tone}">
        <kindMeta.icon size={12} strokeWidth={2} />
      </div>
      <span class="truncate text-[13px] font-medium">{item.agentName}</span>
      <span class="text-[10px] uppercase tracking-wide text-muted-foreground">{kindMeta.label}</span>
      {#if item.risk}
        <RiskBadge level={item.risk.level} />
      {/if}
      <div class="ml-auto">
        <InboxUrgencyBadge suspendedAt={item.suspendedAt} thresholdMs={urgencyThresholdMs} />
      </div>
    </div>
    <p class="truncate text-[12px] text-muted-foreground" title={item.summary}>
      {item.summary}
    </p>
    {#if item.toolName}
      <code class="text-[10px] text-muted-foreground/80">{item.toolName}</code>
    {/if}
  </div>
</div>
