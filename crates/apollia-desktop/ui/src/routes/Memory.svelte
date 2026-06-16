<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { MemoryEntry, MemorySearchResult } from "$lib/types";
  import MemorySearch from "../components/memory/MemorySearch.svelte";
  import NamespaceSidebar, { type NamespaceCategory } from "../components/memory/NamespaceSidebar.svelte";
  import MemoryEntryRow from "../components/memory/MemoryEntryRow.svelte";
  import MemoryEntrySheet from "../components/memory/MemoryEntrySheet.svelte";
  import EmptyState from "../components/common/EmptyState.svelte";
  import { Database, Clock, Cog, Search, UserCircle2 } from "lucide-svelte";
  import { LoadingShimmer } from "$lib/components/feedback";
  import { addToast } from "$lib/components/ui/toast/store";
  import { PageHeader, SplitLayout } from "$lib/components/operator";
  import { Button } from "$lib/components/ui/button";
  import { TabBar } from "$lib/components/ui/tabs";

  // The per-user profile is edited from `Paramètres → Profil`.
  // This page is the namespace explorer only (sidebar with classified
  // namespaces + entries list + detail sheet).  Selecting `__user__` shows
  // a banner redirecting to the Profile settings page.

  type EntryTypeFilter = "all" | "episodic" | "semantic" | "procedural";

  // ── Memory state ────────────────────────────────────────────────────────────
  let installedAgentNames = $state<Set<string>>(new Set());

  let namespaces = $state<{
    name: string;
    count?: number;
    category: NamespaceCategory;
    displayName?: string;
  }[]>([]);
  let selectedNamespace = $state<string>("");
  let entries = $state<MemoryEntry[]>([]);
  let searchQuery = $state("");
  let searching = $state(false);
  let loadingMemory = $state(true);
  let typeFilter = $state<EntryTypeFilter>("all");
  let selectedEntry = $state<MemoryEntry | null>(null);

  // ── Filtered entries (par type d'entrée) ─────────────────────────────────────
  const filteredEntries = $derived.by(() => {
    if (typeFilter === "all") return entries;
    return entries.filter((e) => e.entry_type === typeFilter);
  });

  // Compteurs par type pour les chips.
  const entryCounts = $derived.by(() => {
    const counts = { all: entries.length, episodic: 0, semantic: 0, procedural: 0 };
    for (const e of entries) {
      if (e.entry_type === "episodic") counts.episodic++;
      else if (e.entry_type === "semantic") counts.semantic++;
      else if (e.entry_type === "procedural") counts.procedural++;
    }
    return counts;
  });

  const showProfileBanner = $derived(selectedNamespace === "__user__");

  // ── Classification des namespaces ────────────────────────────────────────────
  function classifyNamespace(name: string, agents: Set<string>): NamespaceCategory {
    if (name === "__user__") return "user";
    if (name.includes(":")) return "project";
    if (agents.has(name)) return "agent";
    return "other";
  }

  function displayNameFor(name: string, category: NamespaceCategory): string | undefined {
    if (category === "user") return $t("memory.user_profile");
    if (category === "project") {
      const [projectId, sub] = name.split(":", 2);
      return sub ? `${sub} · ${projectId}` : projectId;
    }
    return undefined;
  }

  // ── Memory functions ────────────────────────────────────────────────────────
  async function loadNamespaces(): Promise<void> {
    try {
      const [names, agents] = await Promise.all([
        invoke<string[]>("list_memory_namespaces"),
        invoke<Array<{ name: string }>>("list_agents").catch(() => []),
      ]);
      installedAgentNames = new Set(agents.map((a) => a.name));
      namespaces = names.map((name) => {
        const category = classifyNamespace(name, installedAgentNames);
        return {
          name,
          category,
          displayName: displayNameFor(name, category),
        };
      });
      if (namespaces.length > 0 && selectedNamespace === "") {
        selectedNamespace = namespaces[0].name;
      }
    } catch (e) {
      addToast(`${$t("memory.load_namespaces_failed")}: ${e}`, "error");
    }
  }

  async function loadEntries(): Promise<void> {
    if (selectedNamespace === "") return;
    loadingMemory = true;
    try {
      entries = await invoke("list_memory_entries", { namespace: selectedNamespace });
      searching = false;
      namespaces = namespaces.map((ns) =>
        ns.name === selectedNamespace ? { ...ns, count: entries.length } : ns
      );
    } catch (e) {
      addToast(`${$t("memory.load_entries_failed")}: ${e}`, "error");
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
      addToast(`${$t("memory.search_failed")}: ${e}`, "error");
    } finally {
      loadingMemory = false;
    }
  }

  async function handleNamespaceChange(ns: string): Promise<void> {
    selectedNamespace = ns;
    searchQuery = "";
    searching = false;
    typeFilter = "all";
    selectedEntry = null;
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
        addToast($t("memory.entry_deleted"), "success");
      } else {
        addToast($t("memory.entry_not_found"), "error");
      }
    } catch (e) {
      addToast(`${$t("memory.delete_failed")}: ${e}`, "error");
    }
  }

  async function handleCopyEntry(entry: MemoryEntry): Promise<void> {
    try {
      await navigator.clipboard.writeText(entry.value);
      addToast($t("memory.value_copied"), "success");
    } catch {
      addToast($t("memory.copy_failed"), "error");
    }
  }

  // ── Type filter tabs (underline pattern aligné sur Projets) ────────────────
  type FilterChipDef = { key: EntryTypeFilter; labelKey: string; Icon: typeof Database };
  const filterChips: FilterChipDef[] = [
    { key: "all", labelKey: "memory.filter_all", Icon: Database },
    { key: "episodic", labelKey: "memory.type.episodic", Icon: Clock },
    { key: "semantic", labelKey: "memory.type.semantic", Icon: Database },
    { key: "procedural", labelKey: "memory.type.procedural", Icon: Cog },
  ];

  const memoryTabItems = $derived(
    filterChips.map((c) => ({ key: c.key, label: $t(c.labelKey), count: entryCounts[c.key] })),
  );

  onMount(async () => {
    await loadNamespaces();
    if (selectedNamespace !== "") {
      await loadEntries();
    } else {
      loadingMemory = false;
    }
  });
</script>

<div class="flex flex-col h-full" data-testid="memory-page">
  <PageHeader
    kicker={$t("memory.kicker")}
    title={$t("memory.title")}
    subtitle={$t("memory.subtitle")}
  />

  <div class="flex flex-1 min-h-0 mt-4 border-t border-border/40 overflow-hidden">
    {#if !loadingMemory && namespaces.length === 0}
      <div class="flex-1 px-8 py-8">
        <EmptyState
          icon={Database}
          title={$t("memory.empty_title")}
          subtitle={$t("memory.empty_subtitle")}
          page="memory"
        />
      </div>
    {:else}
      <SplitLayout sidebarTestid="namespace-sidebar" sidebarClass="bg-surface-1/40 border-border/60">
        {#snippet sidebar()}
          <!-- Sidebar : namespaces classifiés -->
          <NamespaceSidebar
            namespaces={namespaces}
            selected={selectedNamespace}
            loading={loadingMemory && namespaces.length === 0}
            onselect={handleNamespaceChange}
          />
        {/snippet}

      <!-- Main : filtres + entries -->
      <main class="flex-1 flex flex-col min-w-0">
        {#if showProfileBanner}
          <div class="mx-6 mt-4 rounded-md border border-primary/30 bg-primary/5 px-3 py-2 flex items-start gap-2">
            <UserCircle2 size={14} class="text-primary mt-0.5" />
            <p class="text-[11.5px] text-muted-foreground leading-snug">
              {$t("memory.profile_banner_prefix")}
              <a href="/settings/profile" class="text-primary underline-offset-2 hover:underline font-medium">
                {$t("memory.profile_banner_link")}
              </a>.
              {$t("memory.profile_banner_suffix")}
            </p>
          </div>
        {/if}

        <!-- Header : search + namespace breadcrumb -->
        <div class="px-6 pt-4 pb-2 flex items-center gap-4 flex-wrap border-b border-border/30">
          <div class="flex items-center gap-1.5 text-[11.5px] text-muted-foreground min-w-0">
            <Database size={11} class="shrink-0" />
            <span class="font-mono truncate" title={selectedNamespace}>{selectedNamespace || "-"}</span>
            {#if searching}
              <span class="ml-2 inline-flex items-center gap-1 text-info">
                <Search size={10} />
                {$t("memory.results_count", { values: { count: filteredEntries.length } })}
              </span>
            {/if}
          </div>
          <div class="ml-auto flex-1 max-w-[320px] min-w-[180px]">
            <MemorySearch value={searchQuery} onsearch={handleSearch} />
          </div>
        </div>

        <!-- Type tabs (underline pattern, aligné Projets) -->
        <div class="px-6 pt-2">
          <TabBar
            variant="underline"
            items={memoryTabItems}
            activeTab={typeFilter}
            ontabchange={(k) => (typeFilter = k as EntryTypeFilter)}
            testidPrefix="memory-type"
          />
        </div>

        <!-- Entries list -->
        <div class="flex-1 overflow-y-auto" data-testid="memory-entries-list">
          {#if loadingMemory}
            <div class="px-6 py-4 space-y-2">
              {#each Array(5) as _}
                <LoadingShimmer width="100%" height="3rem" />
              {/each}
            </div>
          {:else if filteredEntries.length === 0}
            <div class="flex flex-col items-center justify-center gap-2 py-16 text-center">
              <div class="w-10 h-10 rounded-full bg-muted flex items-center justify-center">
                {#if searching}
                  <Search size={16} class="text-muted-foreground" />
                {:else}
                  <Database size={16} class="text-muted-foreground" />
                {/if}
              </div>
              <p class="text-[12.5px] text-muted-foreground">
                {searching
                  ? $t("memory.no_search_results")
                  : typeFilter !== "all"
                    ? $t("memory.empty_entries_by_type", {
                        values: {
                          type: $t(
                            filterChips.find((c) => c.key === typeFilter)?.labelKey ?? "memory.filter_all",
                          ).toLowerCase(),
                        },
                      })
                    : $t("memory.empty_entries")}
              </p>
              {#if typeFilter !== "all"}
                <Button variant="ghost" size="sm"
                  type="button"
                  onclick={() => (typeFilter = "all")}
                  class="text-[11.5px] text-primary hover:underline"
                >
                  {$t("memory.show_all_entries")}
                </Button>
              {/if}
            </div>
          {:else}
            {#each filteredEntries as entry (entry.id)}
              <MemoryEntryRow
                {entry}
                searching={searching}
                selected={selectedEntry?.id === entry.id}
                onclick={() => (selectedEntry = entry)}
                oncopy={() => handleCopyEntry(entry)}
                ondelete={() => handleDelete(entry.id)}
              />
            {/each}
          {/if}
        </div>
      </main>
      </SplitLayout>
    {/if}
  </div>
</div>

<!-- Memory entry detail Sheet -->
<MemoryEntrySheet
  entry={selectedEntry}
  namespace={selectedNamespace}
  open={selectedEntry !== null}
  onclose={() => (selectedEntry = null)}
  ondelete={async (id) => {
    await handleDelete(id);
    selectedEntry = null;
  }}
/>
