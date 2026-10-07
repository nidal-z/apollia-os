<script lang="ts">
  /**
   * The row of tabs of the split chat in a narrow window: one tab per pane,
   * the active one marked.
   *
   * The row always scrolls sideways (wheel, drag, arrow keys); whether it
   * draws a scroll bar when the tabs overflow is the reader's choice in
   * Settings > Appearance. The tab of the active pane is brought back into
   * the visible part of the row whenever it changes.
   */
  import { tick } from "svelte";
  import { t } from "svelte-i18n";
  import { arrowTab, revealIn, scrollRailClass, tabRail } from "$lib/chat/paneScroll";

  export interface PaneTab {
    id: string;
    title: string;
  }

  interface Props {
    tabs: PaneTab[];
    activeId: string;
    /** Whether the row draws a scroll bar when its tabs overflow. */
    showScrollbar: boolean;
    onactivate: (id: string) => void;
  }

  let { tabs, activeId, showScrollbar, onactivate }: Props = $props();

  let rail = $state<HTMLElement | null>(null);

  function tabOf(id: string): HTMLElement | null {
    if (!rail) return null;
    for (const el of rail.querySelectorAll<HTMLElement>("[data-pane-tab]")) {
      if (el.dataset.paneTab === id) return el;
    }
    return null;
  }

  $effect(() => {
    const id = activeId;
    // The count is read so that a tab added or removed re-runs the reveal.
    void tabs.length;
    void tick().then(() => {
      const tab = tabOf(id);
      if (rail && tab) revealIn(rail, tab);
    });
  });

  function onKeydown(e: KeyboardEvent): void {
    const next = arrowTab(
      tabs.map((tab) => tab.id),
      activeId,
      e.key,
    );
    if (next === null) return;
    e.preventDefault();
    onactivate(next);
    void tick().then(() => tabOf(next)?.focus());
  }
</script>

<div
  bind:this={rail}
  use:tabRail
  class="flex shrink-0 items-center gap-1 border-b border-border px-3 {scrollRailClass(
    showScrollbar,
  )}"
  role="tablist"
  tabindex={-1}
  aria-label={$t("chat.panes.tabs_aria")}
  onkeydown={onKeydown}
  data-scrollbar={showScrollbar ? "shown" : "hidden"}
  data-testid="chat-pane-tabs"
>
  {#each tabs as tab (tab.id)}
    {@const active = tab.id === activeId}
    <button
      type="button"
      role="tab"
      aria-selected={active}
      tabindex={active ? 0 : -1}
      data-pane-tab={tab.id}
      class="max-w-48 shrink-0 truncate border-b-2 px-3 py-2 text-body-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/60 {active
        ? 'border-primary font-medium text-foreground'
        : 'border-transparent text-muted-foreground hover:text-foreground'}"
      onclick={() => onactivate(tab.id)}
      data-testid="chat-pane-tab"
    >
      {tab.title}
    </button>
  {/each}
</div>
