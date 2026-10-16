<script lang="ts">
  /**
   * The detail sidebar: a collapsible column beside the screen, on the side
   * opposite the navigation bar (the right unless Appearance says otherwise), that
   * carries the contextual tabs of the current screen (the entry the journal
   * shows more of, the item a batch shows the detail of, the model the AI
   * screen opens the panel of), the Guide tab last and on every screen. Opened and collapsed from the top bar, by Cmd+/ (Ctrl+/
   * elsewhere) on the Guide tab, and from its own collapse icon. Under the
   * `md` breakpoint it covers the window so the guide stays usable. While the
   * onboarding veil is shown it rises above it, with the top bar.
   */
  import { t } from "svelte-i18n";
  import { PanelLeftClose, PanelRightClose } from "lucide-svelte";
  import { navPosition } from "$lib/stores/chatAppearance";
  import { navSide } from "$lib/layout/navSide";
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
  import { aiModelSelection } from "$lib/stores/aiModelPanel";
  import GuideTab from "../../../components/companion/GuideTab.svelte";
  import JournalEntryTab from "../../../components/journal/JournalEntryTab.svelte";
  import ModelPanel from "../../../components/ai/ModelPanel.svelte";

  const fullscreen = $derived($sidebarState === "drawer");
  const side = $derived(navSide($navPosition));

  // The journal offers its contextual tab while one of its entries is open.
  const entryOffered = $derived($journalSelection !== null);
  // The AI screen offers its own while the panel of one of its models is open.
  const modelOffered = $derived($aiModelSelection !== null);
  const activeTab = $derived(
    shownTab($detailSidebar.tab, entryOffered, modelOffered),
  );

  const tabs = $derived([
    ...(entryOffered ? [{ key: "entry", label: $t("journal.tab_entry") }] : []),
    ...(modelOffered ? [{ key: "model", label: $t("ai.panel.sidebar_tab") }] : []),
    { key: "guide", label: $t("detailSidebar.tab_guide") },
  ]);

  function showTab(key: string): void {
    if (key === "entry") detailSidebar.openEntry();
    else if (key === "model") detailSidebar.openModel();
    else detailSidebar.openGuide();
  }
</script>

{#if $detailSidebar.open}
  <aside
    class="flex shrink-0 flex-col overflow-hidden {side.detailBorder} border-border bg-surface-1"
    class:relative={!fullscreen}
    class:detail-sidebar-fullscreen={fullscreen}
    style:width={fullscreen ? undefined : `${DETAIL_SIDEBAR_WIDTH_REM}rem`}
    style:z-index={$onboardingModalOpen ? zLayer.onboardingAbove : undefined}
    aria-label={$t("detailSidebar.aria_label")}
    data-testid="detail-sidebar"
    data-tab={activeTab}
    data-side={side.detail}
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
        {#if side.detail === "left"}
          <PanelLeftClose size={14} strokeWidth={1.5} />
        {:else}
          <PanelRightClose size={14} strokeWidth={1.5} />
        {/if}
      </Button>
    </div>

    <div class="min-h-0 flex-1 overflow-hidden">
      {#if activeTab === "entry"}
        <JournalEntryTab />
      {:else if activeTab === "model"}
        <ModelPanel />
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
