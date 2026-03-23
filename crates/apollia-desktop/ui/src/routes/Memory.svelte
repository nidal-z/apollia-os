<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { MemoryEntry, MemorySearchResult } from "$lib/types";
  import NamespaceSelector from "../components/memory/NamespaceSelector.svelte";
  import MemorySearch from "../components/memory/MemorySearch.svelte";
  import { Database } from "lucide-svelte";
  import MemoryTable from "../components/memory/MemoryTable.svelte";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { addToast } from "$lib/components/ui/toast/store";
  import EmptyState from "../components/common/EmptyState.svelte";

  let namespaces = $state<string[]>([]);
  let selectedNamespace = $state<string>("");
  let entries = $state<MemoryEntry[]>([]);
  let searchQuery = $state("");
  let searching = $state(false);
  let loading = $state(true);

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
    loading = true;
    try {
      entries = await invoke("list_memory_entries", { namespace: selectedNamespace });
      searching = false;
    } catch (e) {
      addToast(`${$t('memory.load_entries_failed')}: ${e}`, "error");
    } finally {
      loading = false;
    }
  }

  async function handleSearch(query: string): Promise<void> {
    searchQuery = query;

    if (query === "") {
      await loadEntries();
      return;
    }

    loading = true;
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
      loading = false;
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

  onMount(async () => {
    await loadNamespaces();
    if (selectedNamespace !== "") {
      await loadEntries();
    } else {
      loading = false;
    }
  });
</script>

<div class="max-w-6xl space-y-6" data-testid="memory-page">
  <!-- Header -->
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-semibold">{$t('memory.title')}</h1>
      <p class="text-xs text-muted-foreground" data-testid="memory-subtitle">{$t('memory.subtitle')}</p>
    </div>
  </div>

  <!-- AC-6: No namespaces -->
  {#if !loading && namespaces.length === 0}
    <EmptyState
      icon={Database}
      title={$t('memory.empty_title')}
      subtitle={$t('memory.empty_subtitle')}
      page="memory"
    />
  {:else}
    <!-- Controls: Namespace selector + Search -->
    <div class="flex items-center gap-4">
      <NamespaceSelector
        {namespaces}
        selected={selectedNamespace}
        onselect={handleNamespaceChange}
      />
      <MemorySearch value={searchQuery} onsearch={handleSearch} />
    </div>

    <!-- Table -->
    {#if loading}
      <div class="space-y-2 py-4">
        <Skeleton width="100%" height="2.5rem" />
        <Skeleton width="100%" height="2.5rem" />
        <Skeleton width="100%" height="2.5rem" />
        <Skeleton width="80%" height="2.5rem" />
      </div>
    {:else}
      <MemoryTable {entries} {searching} ondelete={handleDelete} />
    {/if}
  {/if}
</div>
