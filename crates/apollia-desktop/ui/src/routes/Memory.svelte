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
  import { PageHeader } from "$lib/components/operator";

  // ADR-087 — the per-user profile is edited from `Paramètres → Profil`.
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
    if (category === "user") return "Profil utilisateur";
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
      addToast("Valeur copiée", "success");
    } catch {
      addToast("Impossible de copier", "error");
    }
  }

  // ── Type filter chips ───────────────────────────────────────────────────────
  type FilterChipDef = { key: EntryTypeFilter; label: string; Icon: typeof Database };
  const filterChips: FilterChipDef[] = [
    { key: "all", label: "Toutes", Icon: Database },
    { key: "episodic", label: "Épisodique", Icon: Clock },
    { key: "semantic", label: "Sémantique", Icon: Database },
    { key: "procedural", label: "Procédurale", Icon: Cog },
  ];

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
    kicker="MÉMOIRE"
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
      <!-- Sidebar : namespaces classifiés -->
      <NamespaceSidebar
        namespaces={namespaces}
        selected={selectedNamespace}
        loading={loadingMemory && namespaces.length === 0}
        onselect={handleNamespaceChange}
      />

      <!-- Main : filtres + entries -->
      <main class="flex-1 flex flex-col min-w-0">
        {#if showProfileBanner}
          <div class="mx-6 mt-4 rounded-md border border-primary/30 bg-primary/5 px-3 py-2 flex items-start gap-2">
            <UserCircle2 size={14} class="text-primary mt-0.5" />
            <p class="text-[11.5px] text-muted-foreground leading-snug">
              Le profil utilisateur s'édite dans
              <a href="/settings/profile" class="text-primary underline-offset-2 hover:underline font-medium">
                Paramètres → Profil
              </a>.
              Cette vue est en lecture seule pour le debug.
            </p>
          </div>
        {/if}

        <!-- Header : type chips + search -->
        <div class="px-6 py-3 border-b border-border/40 bg-surface-1/30">
          <div class="flex items-center gap-3 flex-wrap">
            <div class="inline-flex items-center gap-1 p-0.5 rounded-md bg-muted/40 border border-border/40">
              {#each filterChips as chip}
                {@const isActive = typeFilter === chip.key}
                {@const count = entryCounts[chip.key]}
                {@const Icon = chip.Icon}
                <button
                  type="button"
                  onclick={() => (typeFilter = chip.key)}
                  class="inline-flex items-center gap-1.5 px-2.5 py-1 rounded text-[11.5px] font-medium tracking-tight transition-colors {isActive
                    ? 'bg-background text-foreground shadow-sm'
                    : 'text-muted-foreground hover:text-foreground hover:bg-background/50'}"
                  data-testid="memory-type-filter-{chip.key}"
                >
                  <Icon size={11} />
                  {chip.label}
                  <span class="tabular-nums {isActive ? 'text-muted-foreground' : 'text-muted-foreground/60'}">
                    {count}
                  </span>
                </button>
              {/each}
            </div>

            <div class="ml-auto flex-1 max-w-[320px] min-w-[180px]">
              <MemorySearch value={searchQuery} onsearch={handleSearch} />
            </div>
          </div>

          <div class="mt-2 flex items-center gap-1.5 text-[10.5px] text-muted-foreground/80">
            <Database size={10} />
            <span class="font-mono">{selectedNamespace || "—"}</span>
            {#if searching}
              <span class="ml-2 inline-flex items-center gap-1 text-info">
                <Search size={9} />
                {filteredEntries.length} résultat{filteredEntries.length > 1 ? "s" : ""}
              </span>
            {/if}
          </div>
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
                    ? `Aucune entrée de type ${filterChips.find((c) => c.key === typeFilter)?.label.toLowerCase()}`
                    : $t("memory.empty_entries")}
              </p>
              {#if typeFilter !== "all"}
                <button
                  type="button"
                  onclick={() => (typeFilter = "all")}
                  class="text-[11.5px] text-primary hover:underline"
                >
                  Voir toutes les entrées
                </button>
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
