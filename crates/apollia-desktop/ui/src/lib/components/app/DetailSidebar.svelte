<script lang="ts">
  /**
   * The detail sidebar: a collapsible column on the right of the screen that
   * carries the contextual tabs of the current screen (the entry the journal
   * shows more of, the item a batch shows the detail of), the Guide tab last and on every screen. Opened and collapsed from the top bar, by Cmd+/ (Ctrl+/
   * elsewhere) on the Guide tab, and from its own collapse icon. Under the
   * `md` breakpoint it covers the window so the guide stays usable. While the
   * onboarding veil is shown it rises above it, with the top bar.
   */
  import { t } from "svelte-i18n";
  import { PanelRightClose } from "lucide-svelte";
  import { TabBar } from "$lib/components/ui/tabs";
  import { Button } from "$lib/components/ui/button";
  import { sidebarState } from "$lib/stores/layout";
  import { onboardingModalOpen } from "$lib/stores/onboarding";
  import { zLayer } from "$lib/design/tokens";
  import {
    DETAIL_SIDEBAR_WIDTH_REM,
    detailSidebar,
    shownTab,
  } from "$lib/stores/detailSidebar";
  import { journalSelection } from "$lib/stores/journalSelection";
  import { batchItemSelection } from "$lib/stores/batchView";
  import GuideTab from "../../../components/companion/GuideTab.svelte";
  import JournalEntryTab from "../../../components/journal/JournalEntryTab.svelte";
  import BatchItemTab from "../../../components/batch/view/BatchItemTab.svelte";

  const fullscreen = $derived($sidebarState === "drawer");

  // The journal offers its contextual tab while one of its entries is open.
  const entryOffered = $derived($journalSelection !== null);
  // The view of a batch offers its own while one of its items is open.
  const itemOffered = $derived($batchItemSelection !== null);
  const activeTab = $derived(shownTab($detailSidebar.tab, entryOffered, itemOffered));

  const tabs = $derived([
    ...(entryOffered ? [{ key: "entry", label: $t("journal.tab_entry") }] : []),
    ...(itemOffered ? [{ key: "item", label: $t("batch_view.item.tab") }] : []),
    { key: "guide", label: $t("detailSidebar.tab_guide") },
  ]);

  function showTab(key: string): void {
    if (key === "entry") detailSidebar.openEntry();
    else if (key === "item") detailSidebar.openItem();
    else detailSidebar.openGuide();
  }
</script>

{#if $detailSidebar.open}
  <aside
    class="flex shrink-0 flex-col overflow-hidden border-l border-border bg-surface-1"
    class:relative={!fullscreen}
    class:detail-sidebar-fullscreen={fullscreen}
    style:width={fullscreen ? undefined : `${DETAIL_SIDEBAR_WIDTH_REM}rem`}
    style:z-index={$onboardingModalOpen ? zLayer.onboardingAbove : undefined}
    aria-label={$t("detailSidebar.aria_label")}
    data-testid="detail-sidebar"
    data-tab={activeTab}
  >
    <div class="flex shrink-0 items-end gap-2 px-3 pt-2">
      <TabBar
        items={tabs}
        {activeTab}
        ontabchange={showTab}
        testidPrefix="detail-sidebar"
        variant="underline"
        class="min-w-0 flex-1"
      />
      <Button
        variant="ghost"
        size="icon-sm"
        class="mb-1"
        onclick={() => detailSidebar.close()}
        aria-label={$t("detailSidebar.collapse")}
        title={$t("detailSidebar.collapse")}
        data-testid="detail-sidebar-collapse"
      >
        <PanelRightClose size={14} strokeWidth={1.5} />
      </Button>
    </div>

    <div class="min-h-0 flex-1 overflow-hidden">
      {#if activeTab === "entry"}
        <JournalEntryTab />
      {:else if activeTab === "item"}
        <BatchItemTab />
      {:else}
        <GuideTab />
      {/if}
    </div>
  </aside>
{/if}

<style>
  /* Under `md` the column covers the window, the way the floating guide did,
     so the chat stays usable and the collapse icon stays reachable. */
  .detail-sidebar-fullscreen {
    position: fixed;
    inset: 0;
    z-index: var(--z-overlay);
  }
</style>
