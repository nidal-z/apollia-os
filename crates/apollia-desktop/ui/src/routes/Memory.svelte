<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { uiMode } from "$lib/stores/mode";
  import type { MemoryEntry, MemorySearchResult } from "$lib/types";
  import MemorySearch from "../components/memory/MemorySearch.svelte";
  import NamespaceSidebar, { type NamespaceCategory } from "../components/memory/NamespaceSidebar.svelte";
  import MemoryEntryRow from "../components/memory/MemoryEntryRow.svelte";
  import MemoryEntrySheet from "../components/memory/MemoryEntrySheet.svelte";
  import UserMemoryDashboard from "../components/memory/UserMemoryDashboard.svelte";
  import EmptyState from "../components/common/EmptyState.svelte";
  import { Database, Clock, Cog, Search } from "lucide-svelte";
  import { LoadingShimmer } from "$lib/components/feedback";
  import { addToast } from "$lib/components/ui/toast/store";
  import { PageHeader } from "$lib/components/operator";
  import { TabBar } from "$lib/components/ui/tabs";

  // L'onglet "Tools" qui listait les outils runtime en read-only a été retiré ici :
  // sémantiquement les outils n'ont rien à voir avec la mémoire (capacités runtime
  // vs faits/conversations retenus). La page dédiée `settings/Tools.svelte` propose
  // déjà une gestion complète des outils (toggle, configuration, drawer).
  type Tab = "user_memory" | "memory";
  type EntryTypeFilter = "all" | "episodic" | "semantic" | "procedural";

  let activeTab = $state<Tab>("user_memory");

  // ── Memory state ──
  // Set des noms d'agents installés (pour classifier les namespaces).
  // Rempli par `list_agents` IPC au chargement.
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

  // ── Tab labels ──
  let userMemoryLabel = $derived(
    $uiMode === "operator"
      ? $t("memory.user_memory.tab_operator")
      : $t("memory.user_memory.tab_builder")
  );

  let tabItems = $derived.by(() => {
    // 2 tabs identiques en operator et builder :
    // - "user_memory" : mémoire personnelle (chips Préférences/Habitudes/Contexte)
    // - "memory" : explorateur namespace (sidebar avec classification Profil/Agents/Projets)
    // L'ancien tab "tools" a été retiré (doublon avec settings/Tools).
    return [
      { key: "user_memory", label: userMemoryLabel },
      { key: "memory", label: $t("memory.tab_memory") },
    ];
  });

  // ── Filtered entries (par type d'entrée + score) ────────────────────────────
  const filteredEntries = $derived.by(() => {
    if (typeFilter === "all") return entries;
    return entries.filter((e) => e.entry_type === typeFilter);
  });

  // Compteurs par type pour les chips
  const entryCounts = $derived.by(() => {
    const counts = { all: entries.length, episodic: 0, semantic: 0, procedural: 0 };
    for (const e of entries) {
      if (e.entry_type === "episodic") counts.episodic++;
      else if (e.entry_type === "semantic") counts.semantic++;
      else if (e.entry_type === "procedural") counts.procedural++;
    }
    return counts;
  });

  // ── Classification des namespaces ───────────────────────────────────────────
  // Heuristique : on croise avec la liste des agents installés pour identifier
  // les namespaces "agent". Le format `{project_id}:{ns}` (ADR memory scoping)
  // marque les namespaces "project". `__user__` est le profil utilisateur.
  function classifyNamespace(name: string, agents: Set<string>): NamespaceCategory {
    if (name === "__user__") return "user";
    if (name.includes(":")) return "project";
    if (agents.has(name)) return "agent";
    return "other";
  }

  function displayNameFor(name: string, category: NamespaceCategory): string | undefined {
    if (category === "user") return "Profil utilisateur";
    if (category === "project") {
      // Format `{project_id}:{ns}` — extraire la partie projet et la sous-partie
      const [projectId, sub] = name.split(":", 2);
      return sub ? `${sub} · ${projectId}` : projectId;
    }
    return undefined;
  }

  // ── Memory functions ──
  async function loadNamespaces(): Promise<void> {
    try {
      // Charger en parallèle la liste des namespaces ET la liste des agents
      // pour pouvoir classer chaque namespace dès le 1er render.
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
      // Mettre à jour le count du namespace courant (sans casser sa catégorie)
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

  function switchTab(tab: Tab): void {
    activeTab = tab;
    if (tab === "memory" && namespaces.length === 0) {
      loadNamespaces().then(() => {
        if (selectedNamespace !== "") loadEntries();
        else loadingMemory = false;
      });
    }
  }

  function handleTabChange(key: string): void {
    switchTab(key as Tab);
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
    // Charger les namespaces pour que le tab "memory" soit prêt instantanément.
    loadNamespaces().then(() => {
      if (selectedNamespace !== "" && activeTab === "memory") {
        loadEntries();
      } else {
        loadingMemory = false;
      }
    });
  });
</script>

<div class="flex flex-col h-full" data-testid="memory-page">
  <PageHeader
    kicker="MÉMOIRE"
    title={$t("memory.title")}
    subtitle={$t("memory.subtitle")}
  />

  <div class="px-8 pt-6">
    <TabBar
      items={tabItems}
      activeTab={activeTab}
      ontabchange={handleTabChange}
      testidPrefix="memory"
    />
  </div>

  <!-- ═══════════════════════════════════════════════════════════════════════
       User Memory Tab — visible operator + builder
       Chips Préférences/Habitudes/Contexte sur la mémoire personnelle simple.
       ═══════════════════════════════════════════════════════════════════ -->
  {#if activeTab === "user_memory"}
    <div class="px-8 pt-4 pb-8 mx-auto w-full max-w-6xl">
      <UserMemoryDashboard mode={$uiMode} />
    </div>
  {/if}

  <!-- ═══════════════════════════════════════════════════════════════════════
       Memory Tab — operator + builder
       Layout 2-colonnes (sidebar namespaces classifiés + main + sheet détail).
       ═══════════════════════════════════════════════════════════════════ -->
  {#if activeTab === "memory"}
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
        <!-- Sidebar gauche : namespaces classifiés -->
        <NamespaceSidebar
          namespaces={namespaces}
          selected={selectedNamespace}
          loading={loadingMemory && namespaces.length === 0}
          onselect={handleNamespaceChange}
        />

        <!-- Main center : filtres + entries -->
        <main class="flex-1 flex flex-col min-w-0">
          <!-- Header : type chips + search -->
          <div class="px-6 py-3 border-b border-border/40 bg-surface-1/30">
            <div class="flex items-center gap-3 flex-wrap">
              <!-- Type filter chips — outline neutre, pas de multicolor -->
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

              <!-- Search à droite -->
              <div class="ml-auto flex-1 max-w-[320px] min-w-[180px]">
                <MemorySearch value={searchQuery} onsearch={handleSearch} />
              </div>
            </div>

            <!-- Breadcrumb namespace courant -->
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

          <!-- Entries list (scrollable) -->
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
  {/if}
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
