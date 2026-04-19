<script lang="ts">
  import { t } from "svelte-i18n";
  import { Settings2, X } from "lucide-svelte";
  import type { ChatSessionDetail } from "$lib/types";
  import ChatConfigPanelBody from "./ChatConfigPanelBody.svelte";

  interface Props {
    session: ChatSessionDetail | null;
    onupdated: () => void;
    /** Close handler — used by overlay close button; inline rail hides it. */
    onclose?: () => void;
    /** Hide the header bar (inline rail renders its own when embedded in shell). */
    showHeader?: boolean;
  }

  let { session, onupdated, onclose, showHeader = true }: Props = $props();

  /**
   * Drawer tabs. Config is the only active panel today; Metrics/Artifacts/Memory
   * are placeholders for US-SP42-030 / US-SP42-031 and intentionally stubbed so
   * the shape of the API stays stable when those stories land.
   */
  type TabKey = "config" | "metrics" | "artifacts" | "memory";
  let activeTab = $state<TabKey>("config");

  const tabs: { key: TabKey; label: string; testid: string }[] = [
    { key: "config", label: "Config", testid: "context-tab-config" },
    { key: "metrics", label: "Metrics", testid: "context-tab-metrics" },
    { key: "artifacts", label: "Artifacts", testid: "context-tab-artifacts" },
    { key: "memory", label: "Memory", testid: "context-tab-memory" },
  ];
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

  <!-- Tabs -->
  <div
    role="tablist"
    class="flex shrink-0 gap-0.5 border-b border-border/30 px-3 py-1.5 overflow-x-auto scrollbar-none"
    data-testid="context-drawer-tabs"
  >
    {#each tabs as tab (tab.key)}
      <button
        role="tab"
        aria-selected={activeTab === tab.key}
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
  <div class="flex-1 min-h-0 overflow-y-auto">
    {#if activeTab === "config"}
      <ChatConfigPanelBody
        {session}
        {onupdated}
        {onclose}
        showHeader={false}
      />
    {:else}
      <div class="flex h-full items-center justify-center px-6 py-10 text-center">
        <p class="text-xs text-muted-foreground/60 italic" data-testid="context-drawer-placeholder-{activeTab}">
          {activeTab === "metrics" ? "Metrics panel — coming in US-SP42-030" :
           activeTab === "artifacts" ? "Artifacts panel — coming in US-SP42-031" :
           "Memory panel — coming in US-SP42-030"}
        </p>
      </div>
    {/if}
  </div>
</div>
