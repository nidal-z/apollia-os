<script lang="ts">
  /**
   * The detail sidebar: a collapsible column on the right of the screen that
   * carries the contextual tabs of the current screen, the Guide tab last and
   * on every screen. Opened and collapsed from the top bar, by Cmd+/ (Ctrl+/
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
  } from "$lib/stores/detailSidebar";
  import GuideTab from "../../../components/companion/GuideTab.svelte";

  const fullscreen = $derived($sidebarState === "drawer");

  const tabs = $derived([{ key: "guide", label: $t("detailSidebar.tab_guide") }]);
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
    data-tab={$detailSidebar.tab}
  >
    <div class="flex shrink-0 items-end gap-2 px-3 pt-2">
      <TabBar
        items={tabs}
        activeTab={$detailSidebar.tab}
        ontabchange={() => detailSidebar.openGuide()}
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
      <GuideTab />
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
