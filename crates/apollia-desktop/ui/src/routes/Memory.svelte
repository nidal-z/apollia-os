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
  import EmptyState from "../components/common/EmptyState.svelte";

  let namespaces = $state<string[]>([]);
  let selectedNamespace = $state<string>("");
  let entries = $state<MemoryEntry[]>([]);
  let searchQuery = $state("");
  let searching = $state(false);
  let loading = $state(true);

  let toast = $state<{ message: string; type: "success" | "error" } | null>(null);
  let toastTimer = $state<ReturnType<typeof setTimeout> | null>(null);

  function showToast(message: string, type: "success" | "error") {
    if (toastTimer !== null) {
      clearTimeout(toastTimer);
    }
    toast = { message, type };
    toastTimer = setTimeout(() => {
      toast = null;
      toastTimer = null;
    }, 4000);
  }

  async function loadNamespaces(): Promise<void> {
    try {
      namespaces = await invoke("list_memory_namespaces");
      if (namespaces.length > 0 && selectedNamespace === "") {
        selectedNamespace = namespaces[0];
      }
    } catch (e) {
      showToast(`${$t('memory.load_namespaces_failed')}: ${e}`, "error");
    }
  }

  async function loadEntries(): Promise<void> {
    if (selectedNamespace === "") return;
    loading = true;
    try {
      entries = await invoke("list_memory_entries", { namespace: selectedNamespace });
      searching = false;
    } catch (e) {
      showToast(`${$t('memory.load_entries_failed')}: ${e}`, "error");
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
      showToast(`${$t('memory.search_failed')}: ${e}`, "error");
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
        showToast($t('memory.entry_deleted'), "success");
      } else {
        showToast($t('memory.entry_not_found'), "error");
      }
    } catch (e) {
      showToast(`${$t('memory.delete_failed')}: ${e}`, "error");
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

<div class="space-y-6">
  <!-- Header -->
  <div class="space-y-1">
    <h1 class="text-2xl font-bold">{$t('memory.title')}</h1>
    <p class="text-sm text-muted-foreground" data-testid="memory-subtitle">{$t('memory.subtitle')}</p>
  </div>

  <!-- Toast -->
  {#if toast}
    <div
      class="rounded-md border px-4 py-2 text-sm {toast.type === 'success'
        ? 'border-[var(--apollia-success)] bg-[var(--apollia-success)]/10 text-[var(--apollia-success)]'
        : 'border-[hsl(var(--destructive))] bg-[hsl(var(--destructive))]/10 text-[hsl(var(--destructive))]'}"
    >
      {toast.message}
    </div>
  {/if}

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
