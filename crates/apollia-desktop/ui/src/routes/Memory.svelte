<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { uiMode } from "$lib/stores/mode";
  import type { MemoryEntry, MemorySearchResult, ToolSummary } from "$lib/types";
  import NamespaceSelector from "../components/memory/NamespaceSelector.svelte";
  import MemorySearch from "../components/memory/MemorySearch.svelte";
  import MemoryTable from "../components/memory/MemoryTable.svelte";
  import UserMemoryDashboard from "../components/memory/UserMemoryDashboard.svelte";
  import RecentExtractions from "../components/memory/RecentExtractions.svelte";
  import ToolSchemaPanel from "../components/tools/ToolSchemaPanel.svelte";
  import EmptyState from "../components/common/EmptyState.svelte";
  import { Database, Wrench, Brain } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { LoadingShimmer } from "$lib/components/feedback";
  import { addToast } from "$lib/components/ui/toast/store";

  type Tab = "user_memory" | "memory" | "tools";

  let activeTab = $state<Tab>("user_memory");

  // ── Memory state ──
  let namespaces = $state<string[]>([]);
  let selectedNamespace = $state<string>("");
  let entries = $state<MemoryEntry[]>([]);
  let searchQuery = $state("");
  let searching = $state(false);
  let loadingMemory = $state(true);

  // ── Tools state ──
  let tools = $state<ToolSummary[]>([]);
  let loadingTools = $state(true);
  let selectedTool = $state<string | null>(null);

  const KIND_BADGE: Record<string, { variant: "default" | "info" | "warning"; label: string }> = {
    native: { variant: "default", label: "Native" },
    mcp: { variant: "info", label: "MCP" },
    python: { variant: "warning", label: "Python" },
  };

  function kindBadge(kind: string): { variant: "default" | "info" | "warning"; label: string } {
    return KIND_BADGE[kind] ?? { variant: "default", label: kind };
  }

  // ── Tab labels ──
  let userMemoryLabel = $derived(
    $uiMode === "operator"
      ? $t("memory.user_memory.tab_operator")
      : $t("memory.user_memory.tab_builder")
  );

  // ── Memory functions ──
  async function loadNamespaces(): Promise<void> {
    try {
      namespaces = await invoke("list_memory_namespaces");
      if (namespaces.length > 0 && selectedNamespace === "") {
        selectedNamespace = namespaces[0];
      }
    } catch (e) {
      addToast(`${$t('memory.load_namespaces_failed')}: ${e}`, "error");
    }
  }

  async function loadEntries(): Promise<void> {
    if (selectedNamespace === "") return;
    loadingMemory = true;
    try {
      entries = await invoke("list_memory_entries", { namespace: selectedNamespace });
      searching = false;
    } catch (e) {
      addToast(`${$t('memory.load_entries_failed')}: ${e}`, "error");
    } finally {
      loadingMemory = false;
    }
  }

  async function handleSearch(query: string): Promise<void> {
    searchQuery = query;

    if (query === "") {
      await loadEntries();
      return;
    }

    loadingMemory = true;
    try {
      const results: MemorySearchResult[] = await invoke("search_memory", {
        namespace: selectedNamespace,
        query,
      });

      entries = results.map((r) => ({
        id: r.id,
        entry_type: r.entry_type,
        key: r.entry_type,
        value: r.content,
        created_at: r.created_at,
        expires_at: null,
        score: r.score,
      }));
      searching = true;
    } catch (e) {
      addToast(`${$t('memory.search_failed')}: ${e}`, "error");
    } finally {
      loadingMemory = false;
    }
  }

  async function handleNamespaceChange(ns: string): Promise<void> {
    selectedNamespace = ns;
    searchQuery = "";
    searching = false;
    await loadEntries();
  }

  async function handleDelete(entryId: string): Promise<void> {
    try {
      const deleted: boolean = await invoke("delete_memory_entry", {
        namespace: selectedNamespace,
        entryId,
      });
      if (deleted) {
        entries = entries.filter((e) => e.id !== entryId);
        addToast($t('memory.entry_deleted'), "success");
      } else {
        addToast($t('memory.entry_not_found'), "error");
      }
    } catch (e) {
      addToast(`${$t('memory.delete_failed')}: ${e}`, "error");
    }
  }

  // ── Tools functions ──
  async function loadTools(): Promise<void> {
    loadingTools = true;
    try {
      tools = await invoke("list_tools");
    } catch (e) {
      addToast(`${$t('memory.tools_load_failed')}: ${e}`, "error");
    } finally {
      loadingTools = false;
    }
  }

  function selectTool(name: string): void {
    selectedTool = name;
  }

  function switchTab(tab: Tab): void {
    activeTab = tab;
    if (tab === "memory" && namespaces.length === 0) {
      loadNamespaces().then(() => {
        if (selectedNamespace !== "") loadEntries();
        else loadingMemory = false;
      });
    }
    if (tab === "tools" && tools.length === 0 && loadingTools) {
      loadTools();
    }
  }

  onMount(async () => {
    loadTools();
  });
</script>

<div class="mx-auto w-full max-w-6xl space-y-6" data-testid="memory-page">
  <!-- Header -->
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-semibold">{$t('memory.title')}</h1>
      <p class="text-xs text-muted-foreground" data-testid="memory-subtitle">{$t('memory.subtitle')}</p>
    </div>
  </div>

  <!-- Tabs -->
  <div class="flex gap-1 rounded-lg bg-muted/50 p-1 w-fit" data-testid="memory-tabs">
    <button
      class="rounded-md px-3 py-1.5 text-xs font-medium transition-colors {activeTab === 'user_memory' ? 'bg-background text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground'}"
      onclick={() => switchTab("user_memory")}
      data-testid="memory-tab-user-memory"
    >
      <span class="flex items-center gap-1.5">
        <Brain size={13} />
        {userMemoryLabel}
      </span>
    </button>
    <button
      class="rounded-md px-3 py-1.5 text-xs font-medium transition-colors {activeTab === 'memory' ? 'bg-background text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground'}"
      onclick={() => switchTab("memory")}
      data-testid="memory-tab-memory"
    >
      <span class="flex items-center gap-1.5">
        <Database size={13} />
        {$t('memory.tab_memory')}
      </span>
    </button>
    <button
      class="rounded-md px-3 py-1.5 text-xs font-medium transition-colors {activeTab === 'tools' ? 'bg-background text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground'}"
      onclick={() => switchTab("tools")}
      data-testid="memory-tab-tools"
    >
      <span class="flex items-center gap-1.5">
        <Wrench size={13} />
        {$t('memory.tab_tools')}
        {#if !loadingTools}
          <span class="rounded-full bg-muted px-1.5 text-[10px] text-muted-foreground">{tools.length}</span>
        {/if}
      </span>
    </button>
  </div>

  <!-- User Memory Tab -->
  {#if activeTab === "user_memory"}
    <RecentExtractions mode={$uiMode} />
    <UserMemoryDashboard mode={$uiMode} />
  {/if}

  <!-- Memory Tab -->
  {#if activeTab === "memory"}
    {#if !loadingMemory && namespaces.length === 0}
      <EmptyState
        icon={Database}
        title={$t('memory.empty_title')}
        subtitle={$t('memory.empty_subtitle')}
        page="memory"
      />
    {:else}
      <div class="flex items-center gap-4">
        <NamespaceSelector
          {namespaces}
          selected={selectedNamespace}
          onselect={handleNamespaceChange}
        />
        <MemorySearch value={searchQuery} onsearch={handleSearch} />
      </div>

      {#if loadingMemory}
        <div class="space-y-2 py-4">
          <LoadingShimmer width="100%" height="2.5rem" />
          <LoadingShimmer width="100%" height="2.5rem" />
          <LoadingShimmer width="100%" height="2.5rem" />
          <LoadingShimmer width="80%" height="2.5rem" />
        </div>
      {:else}
        <MemoryTable {entries} {searching} ondelete={handleDelete} />
      {/if}
    {/if}
  {/if}

  <!-- Tools Tab -->
  {#if activeTab === "tools"}
    {#if loadingTools}
      <div class="space-y-2 py-4">
        <LoadingShimmer width="100%" height="2.5rem" />
        <LoadingShimmer width="100%" height="2.5rem" />
        <LoadingShimmer width="100%" height="2.5rem" />
      </div>
    {:else if tools.length === 0}
      <EmptyState
        icon={Wrench}
        title={$t('memory.tools_empty_title')}
        subtitle={$t('memory.tools_empty_subtitle')}
        page="tools"
      />
    {:else}
      <div class="rounded-xl glass-card glass-border overflow-x-auto" data-testid="tools-list">
        <table class="w-full min-w-[480px] text-sm">
          <thead>
            <tr class="border-b border-border/50 text-left text-xs text-muted-foreground/60 uppercase tracking-wider">
              <th class="px-4 py-3 font-medium">{$t('memory.tools_col_name')}</th>
              <th class="px-4 py-3 font-medium">{$t('memory.tools_col_kind')}</th>
              <th class="px-4 py-3 font-medium">{$t('memory.tools_col_version')}</th>
              <th class="px-4 py-3 font-medium hidden sm:table-cell">{$t('memory.tools_col_description')}</th>
            </tr>
          </thead>
          <tbody>
            {#each tools as tool (tool.name)}
              {@const badge = kindBadge(tool.kind)}
              <tr
                class="border-b border-border/20 cursor-pointer transition-colors hover:bg-muted/30"
                onclick={() => selectTool(tool.name)}
                data-testid="tool-row"
              >
                <td class="px-4 py-3 font-medium">{tool.name}</td>
                <td class="px-4 py-3">
                  <Badge variant={badge.variant}>{badge.label}</Badge>
                </td>
                <td class="px-4 py-3 text-muted-foreground">{tool.version}</td>
                <td class="px-4 py-3 text-muted-foreground truncate max-w-xs hidden sm:table-cell">{tool.description}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  {/if}
</div>

<!-- Tool Schema Panel (Sheet) -->
<ToolSchemaPanel
  toolName={selectedTool ?? ""}
  open={selectedTool !== null}
  onclose={() => selectedTool = null}
/>
