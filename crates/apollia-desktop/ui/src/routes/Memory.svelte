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
  import { Database, Wrench } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { DataTable, type Column } from "$lib/components/ui/data-table";
  import { LoadingShimmer } from "$lib/components/feedback";
  import { addToast } from "$lib/components/ui/toast/store";
  import { PageHeader, Card } from "$lib/components/operator";
  import { TabBar } from "$lib/components/ui/tabs";

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

  let tabItems = $derived.by(() => {
    const items = [{ key: "user_memory", label: userMemoryLabel }];
    if ($uiMode === "builder") {
      items.push(
        { key: "memory", label: $t("memory.tab_memory") },
        { key: "tools", label: $t("memory.tab_tools") },
      );
    }
    return items;
  });

  $effect(() => {
    if ($uiMode !== "builder" && (activeTab === "memory" || activeTab === "tools")) {
      activeTab = "user_memory";
    }
  });

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

  function handleTabChange(key: string): void {
    switchTab(key as Tab);
  }

  onMount(async () => {
    loadTools();
  });
</script>

<div class="mx-auto w-full max-w-6xl" data-testid="memory-page">
  <PageHeader
    kicker="MÉMOIRE"
    title={$t('memory.title')}
    subtitle={$t('memory.subtitle')}
  />

  <div class="px-8 pt-6">
    <TabBar
      items={tabItems}
      activeTab={activeTab}
      ontabchange={handleTabChange}
      testidPrefix="memory"
    />
  </div>

  <!-- User Memory Tab — visible operator + builder -->
  {#if activeTab === "user_memory"}
    <div class="px-8 pt-4 space-y-6">
      <RecentExtractions mode={$uiMode} />
      <UserMemoryDashboard mode={$uiMode} />
    </div>
  {/if}

  <!-- Memory Tab — Builder only -->
  {#if activeTab === "memory"}
    <div class="px-8 pt-4 space-y-4">
      {#if !loadingMemory && namespaces.length === 0}
        <EmptyState
          icon={Database}
          title={$t('memory.empty_title')}
          subtitle={$t('memory.empty_subtitle')}
          page="memory"
        />
      {:else}
        <Card class="overflow-hidden">
          <div class="px-5 pt-4 pb-3 flex items-center gap-4 border-b border-border/40">
            <NamespaceSelector
              {namespaces}
              selected={selectedNamespace}
              onselect={handleNamespaceChange}
            />
            <MemorySearch value={searchQuery} onsearch={handleSearch} />
          </div>
          <div class="p-4">
            {#if loadingMemory}
              <div class="space-y-2">
                <LoadingShimmer width="100%" height="2.5rem" />
                <LoadingShimmer width="100%" height="2.5rem" />
                <LoadingShimmer width="100%" height="2.5rem" />
                <LoadingShimmer width="80%" height="2.5rem" />
              </div>
            {:else}
              <MemoryTable {entries} {searching} ondelete={handleDelete} />
            {/if}
          </div>
        </Card>
      {/if}
    </div>
  {/if}

  <!-- Tools Tab — Builder only -->
  {#if activeTab === "tools"}
    <div class="px-8 pt-4">
    {#if !loadingTools && tools.length === 0}
      <EmptyState
        icon={Wrench}
        title={$t('memory.tools_empty_title')}
        subtitle={$t('memory.tools_empty_subtitle')}
        page="tools"
      />
    {:else}
      {#snippet nameCell(tool: ToolSummary)}
        <span class="font-medium text-foreground">{tool.name}</span>
      {/snippet}
      {#snippet kindCell(tool: ToolSummary)}
        {@const badge = kindBadge(tool.kind)}
        <Badge variant={badge.variant}>{badge.label}</Badge>
      {/snippet}
      {#snippet versionCell(tool: ToolSummary)}
        <span class="text-muted-foreground">{tool.version}</span>
      {/snippet}
      {#snippet descCell(tool: ToolSummary)}
        <span class="text-muted-foreground truncate block max-w-xs">{tool.description}</span>
      {/snippet}

      {@const toolColumns = [
        { key: "name", header: $t("memory.tools_col_name"), sortable: true, cell: nameCell },
        { key: "kind", header: $t("memory.tools_col_kind"), sortable: true, width: "9rem", cell: kindCell },
        { key: "version", header: $t("memory.tools_col_version"), sortable: true, width: "8rem", cell: versionCell },
        { key: "description", header: $t("memory.tools_col_description"), hideOn: "sm", cell: descCell },
      ] as Array<Column<ToolSummary>>}

      <Card class="overflow-hidden">
        <DataTable
          data={tools}
          columns={toolColumns}
          rowKey={(t) => t.name}
          loading={loadingTools}
          onrowclick={(t) => selectTool(t.name)}
          emptyLabel={$t('memory.tools_empty_title')}
          class="min-w-0"
        />
      </Card>
    {/if}
    </div>
  {/if}
</div>

<!-- Tool Schema Panel (Sheet) -->
<ToolSchemaPanel
  toolName={selectedTool ?? ""}
  open={selectedTool !== null}
  onclose={() => selectedTool = null}
/>
