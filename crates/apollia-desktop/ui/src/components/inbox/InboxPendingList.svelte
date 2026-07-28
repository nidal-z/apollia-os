<script lang="ts">
  /**
   * Grouped pending list: date sections, keyed rows with list-motion removal,
   * right-click context menus, roving keyboard nav (arrows + j/k), and the
   * focal HITL / ask_user detail that unfolds under the selected row.
   */
  import { flip } from "svelte/animate";
  import { fly } from "svelte/transition";
  import { t } from "svelte-i18n";
  import { SectionTitle, ListPanel, InboxRow, HITLCard } from "$lib/components/operator";
  import { listNavigation } from "$lib/components/operator/listNavigation";
  import type { AlwaysScope } from "$lib/components/operator/HITLCard.svelte";
  import { listFlip, rowOut } from "$lib/design/listMotion";
  import { ContextMenu } from "$lib/components/ui/context-menu";
  import AskUserForm from "./AskUserForm.svelte";
  import type { AskUserAnswer } from "$lib/types";
  import type { InboxItem } from "./types";
  import {
    GROUP_ORDER,
    rowType,
    riskLevel,
    isChatToolItem,
    questionsForForm,
    contextForForm,
    type GroupKey,
    type Translate,
  } from "./inboxModel";
  import { buildRowMenu } from "./rowMenu";

  interface Props {
    grouped: Record<GroupKey, InboxItem[]>;
    expandedId: string | null;
    submitting: boolean;
    relTime: (iso: string) => string;
    onToggleExpand: (item: InboxItem) => void;
    onApprove: (item: InboxItem) => void;
    onReject: (item: InboxItem) => void;
    onAlwaysAccept: (item: InboxItem, scope: AlwaysScope) => void;
    onRespondAskUser: (item: InboxItem, answers: AskUserAnswer[]) => void;
    onRejectAskUser: (item: InboxItem, reason: string) => void;
    onOpenChat: (sessionId: string) => void;
    onCopyId: (item: InboxItem) => void;
  }

  let {
    grouped,
    expandedId,
    submitting,
    relTime,
    onToggleExpand,
    onApprove,
    onReject,
    onAlwaysAccept,
    onRespondAskUser,
    onRejectAskUser,
    onOpenChat,
    onCopyId,
  }: Props = $props();

  const flat = $derived(GROUP_ORDER.flatMap((g) => grouped[g]));
  const menuHandlers = $derived({
    tr: $t as Translate,
    onApprove,
    onReject,
    onExpand: onToggleExpand,
    onOpenChat,
    onCopyId,
  });

  let container = $state<HTMLElement | null>(null);
  const ROW_SELECTOR = "[data-inbox-nav-row]";

  function rows(): HTMLElement[] {
    return container ? Array.from(container.querySelectorAll<HTMLElement>(ROW_SELECTOR)) : [];
  }

  function focusedItem(): InboxItem | undefined {
    const el = (document.activeElement as HTMLElement | null)?.closest<HTMLElement>(ROW_SELECTOR);
    const id = el?.dataset.itemId;
    return id ? flat.find((i) => i.id === id) : undefined;
  }

  function move(delta: number): void {
    const list = rows();
    if (list.length === 0) return;
    const current = (document.activeElement as HTMLElement | null)?.closest<HTMLElement>(ROW_SELECTOR);
    const idx = current ? list.indexOf(current) : -1;
    const next = Math.max(0, Math.min(list.length - 1, idx + delta));
    list[next]?.focus();
  }

  function isTextTarget(): boolean {
    const tag = (document.activeElement?.tagName ?? "").toLowerCase();
    return tag === "input" || tag === "textarea" || tag === "select";
  }

  function onListKeydown(event: KeyboardEvent): void {
    if (isTextTarget()) return;
    const k = event.key.toLowerCase();
    if (k === "j" || k === "k") {
      event.preventDefault();
      move(k === "j" ? 1 : -1);
    } else if (k === "a") {
      const item = focusedItem();
      if (item && item.kind !== "ask_user") onApprove(item);
    } else if (k === "r") {
      const item = focusedItem();
      if (item && item.kind !== "ask_user") onReject(item);
    }
  }

  function onRowKeydown(event: KeyboardEvent, item: InboxItem): void {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      onToggleExpand(item);
    }
  }

  function onRowClick(event: MouseEvent, item: InboxItem): void {
    if ((event.target as HTMLElement).closest("button, a")) return;
    onToggleExpand(item);
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div bind:this={container} use:listNavigation={{ rowSelector: ROW_SELECTOR }} onkeydown={onListKeydown}>
  {#each GROUP_ORDER as g (g)}
    {#if grouped[g].length > 0}
      <SectionTitle count={grouped[g].length}>{$t(`inbox.group.${g}`)}</SectionTitle>
      <div class="px-8 pb-2">
        <ListPanel>
          {#each grouped[g] as item (item.id)}
            {@const expanded = expandedId === item.id}
            <div animate:flip={listFlip()} out:fly={rowOut()}>
              <ContextMenu items={buildRowMenu(item, menuHandlers)} data-testid="inbox-row-menu">
                <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
                <div
                  data-inbox-nav-row
                  data-item-id={item.id}
                  role="button"
                  tabindex="-1"
                  aria-expanded={expanded}
                  class="rounded-lg outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
                  onclick={(e) => onRowClick(e, item)}
                  onkeydown={(e) => onRowKeydown(e, item)}
                >
                  <InboxRow
                    type={rowType(item)}
                    title={item.summary || "-"}
                    agent={item.agentName}
                    timestamp={relTime(item.suspendedAt)}
                    unread={true}
                    onAction={() => onToggleExpand(item)}
                    onSecondaryAction={item.kind === "ask_user" && item.sessionId
                      ? () => onOpenChat(item.sessionId as string)
                      : undefined}
                    secondaryLabel={item.kind === "ask_user" ? $t("inbox.row.open_conversation") : undefined}
                  />
                </div>
              </ContextMenu>

              {#if expanded}
                <div class="border-b border-border/60 bg-muted/20 px-3.5 py-3">
                  {#if item.kind === "ask_user"}
                    <AskUserForm
                      questions={questionsForForm(item.questions)}
                      context={contextForForm(item)}
                      sessionId={item.sessionId}
                      {submitting}
                      onsubmit={(answers) => onRespondAskUser(item, answers)}
                      oncancel={(reason) => onRejectAskUser(item, reason)}
                      onOpenChat={item.sessionId ? () => onOpenChat(item.sessionId as string) : undefined}
                    />
                  {:else}
                    <div
                      class="rounded-xl bg-gradient-to-br from-[hsl(var(--grad-a)/0.55)] to-[hsl(var(--grad-b)/0.3)] p-px shadow-elev-2"
                    >
                      <HITLCard
                        agent={item.agentName}
                        action={item.summary || $t("inbox.title_operator")}
                        risk={riskLevel(item)}
                        tool={item.toolName}
                        scope={item.kind === "task" ? item.risk?.impact : undefined}
                        summary={item.kind === "task" ? item.risk?.rationale : undefined}
                        params={item.kind === "task" ? (item.risk?.consequences ?? []) : []}
                        onApprove={() => onApprove(item)}
                        onReject={() => onReject(item)}
                        onAlwaysAccept={isChatToolItem(item) ? (s) => onAlwaysAccept(item, s) : undefined}
                        hasProject={false}
                      />
                    </div>
                  {/if}
                </div>
              {/if}
            </div>
          {/each}
        </ListPanel>
      </div>
    {/if}
  {/each}
</div>
