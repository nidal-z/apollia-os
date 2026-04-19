<script lang="ts">
  import { t } from "svelte-i18n";
  import { Settings2, X } from "lucide-svelte";
  import type { ChatSessionDetail } from "$lib/types";
  import ChatConfigPanelBody from "./ChatConfigPanelBody.svelte";
  import SessionMetricsPanel from "./SessionMetricsPanel.svelte";
  import MemoryInjectedPanel from "./MemoryInjectedPanel.svelte";
  import ArtifactsPanel from "./ArtifactsPanel.svelte";
  import { contextDrawerActiveTab } from "$lib/stores/chatLayout";

  interface Props {
    session: ChatSessionDetail | null;
    onupdated: () => void;
    /** Close handler — used by overlay close button; inline rail hides it. */
    onclose?: () => void;
    /** Hide the header bar (inline rail renders its own when embedded in shell). */
    showHeader?: boolean;
  }

  let { session, onupdated, onclose, showHeader = true }: Props = $props();

  type TabKey = "config" | "metrics" | "memory" | "artifacts";
  // Bind the local state to the shared store so global shortcuts
  // (e.g. Cmd+Shift+A) can jump to a specific tab from anywhere.
  let activeTab = $state<TabKey>($contextDrawerActiveTab);
  $effect(() => {
    contextDrawerActiveTab.set(activeTab);
  });
  $effect(() => {
    const v = $contextDrawerActiveTab;
    if (v !== activeTab) activeTab = v;
  });

  const tabs: { key: TabKey; label: string; testid: string }[] = [
    { key: "config", label: "Config", testid: "context-tab-config" },
    { key: "metrics", label: "Metrics", testid: "context-tab-metrics" },
    { key: "memory", label: "Memory", testid: "context-tab-memory" },
    { key: "artifacts", label: "Artifacts", testid: "context-tab-artifacts" },
  ];

  /**
   * Keyboard navigation between tabs — ArrowLeft / ArrowRight (WAI-ARIA
   * tablist pattern). Focus moves with the active tab so screen readers
   * announce the new panel.
   */
  function handleTabKey(e: KeyboardEvent): void {
    const idx = tabs.findIndex((t) => t.key === activeTab);
    if (idx < 0) return;
    if (e.key === "ArrowRight" || e.key === "ArrowDown") {
      e.preventDefault();
      activeTab = tabs[(idx + 1) % tabs.length].key;
    } else if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
      e.preventDefault();
      activeTab = tabs[(idx - 1 + tabs.length) % tabs.length].key;
    } else if (e.key === "Home") {
      e.preventDefault();
      activeTab = tabs[0].key;
    } else if (e.key === "End") {
      e.preventDefault();
      activeTab = tabs[tabs.length - 1].key;
    }
  }
</script>

<div class="flex h-full min-h-0 flex-col" data-testid="context-drawer">
  {#if showHeader}
    <div class="flex items-center justify-between border-b border-border/30 px-4 py-2.5">
      <div class="flex items-center gap-2">
        <Settings2 class="h-4 w-4 text-primary" />
        <h3 class="text-[13px] font-medium">{$t("chat.config_title")}</h3>
      </div>
      {#if onclose}
        <button
          onclick={onclose}
          class="h-7 w-7 inline-flex items-center justify-center rounded-md text-muted-foreground/60 hover:text-foreground hover:bg-muted/40 transition-colors"
          aria-label={$t("a11y.close")}
          data-testid="context-drawer-close"
        >
          <X size={14} />
        </button>
      {/if}
    </div>
  {/if}

  <!-- Tabs — WAI-ARIA tablist with arrow-key navigation. -->
  <div
    role="tablist"
    aria-label="Context drawer tabs"
    class="flex shrink-0 gap-0.5 border-b border-border/30 px-3 py-1.5 overflow-x-auto scrollbar-none"
    data-testid="context-drawer-tabs"
    onkeydown={handleTabKey}
  >
    {#each tabs as tab (tab.key)}
      <button
        role="tab"
        aria-selected={activeTab === tab.key}
        aria-controls="context-panel-{tab.key}"
        tabindex={activeTab === tab.key ? 0 : -1}
        class="shrink-0 rounded-md px-2.5 py-1 text-[11px] font-medium transition-colors
          {activeTab === tab.key
            ? 'bg-primary/10 text-primary'
            : 'text-muted-foreground hover:text-foreground hover:bg-muted/30'}"
        onclick={() => (activeTab = tab.key)}
        data-testid={tab.testid}
      >
        {tab.label}
      </button>
    {/each}
  </div>

  <!-- Tab panels — scroll inside drawer, not the shell. -->
  <div
    class="flex-1 min-h-0 overflow-y-auto"
    role="tabpanel"
    id="context-panel-{activeTab}"
    aria-labelledby={activeTab}
  >
    {#if activeTab === "config"}
      <ChatConfigPanelBody
        {session}
        {onupdated}
        {onclose}
        showHeader={false}
      />
    {:else if activeTab === "metrics" && session?.id}
      <SessionMetricsPanel sessionId={session.id} />
    {:else if activeTab === "memory" && session?.id}
      <MemoryInjectedPanel sessionId={session.id} />
    {:else if activeTab === "artifacts" && session?.id}
      <ArtifactsPanel sessionId={session.id} />
    {:else if activeTab === "artifacts"}
      <div class="flex h-full items-center justify-center px-6 py-10 text-center">
        <p class="text-xs text-muted-foreground/60 italic">
          Aucune session active.
        </p>
      </div>
    {:else}
      <div class="flex h-full items-center justify-center px-6 py-10 text-center">
        <p class="text-xs text-muted-foreground/60 italic">
          Aucune session active.
        </p>
      </div>
    {/if}
  </div>
</div>
