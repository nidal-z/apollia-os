<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { searchMemory, deleteMemoryEntry } from "$lib/ipc/memory";
  import { listMemoryNamespaces, listMemoryEntries } from "$lib/ipc/projects";
  import { listAgents } from "$lib/ipc/connections";
  import { memoryChanged } from "$lib/stores/sse";
  import type { MemoryEntry, MemorySearchResult } from "$lib/types";
  import MemorySearch from "../components/memory/MemorySearch.svelte";
  import NamespaceSidebar, { type NamespaceCategory } from "../components/memory/NamespaceSidebar.svelte";
  import MemoryEntryRow from "../components/memory/MemoryEntryRow.svelte";
  import MemoryEntrySheet from "../components/memory/MemoryEntrySheet.svelte";
  import NamespaceDataActions from "../components/memory/NamespaceDataActions.svelte";
  import { EmptyState } from "$lib/components/layout";
  import { Database, Clock, Cog, Search, UserCircle2 } from "lucide-svelte";
  import { flip } from "svelte/animate";
  import { fly } from "svelte/transition";
  import { LoadingShimmer } from "$lib/components/feedback";
  import { addToast } from "$lib/components/ui/toast/store";
  import { DetailHeader, SplitLayout, ErrorBanner } from "$lib/components/operator";
  import { listNavigation } from "$lib/components/operator/listNavigation";
  import { Button } from "$lib/components/ui/button";
  import { Disclosure } from "$lib/components/ui/disclosure";
  import { TabBar } from "$lib/components/ui/tabs";
  import { listFlip, rowIn, rowOut } from "$lib/design/listMotion";
  import { contentIn, contentOut } from "$lib/design/routeTransition";
  import { reportError } from "$lib/errors/reportError";
  import type { HumanizedError } from "$lib/errors/humanize";

  // The per-user profile is edited from `Settings -> Profile`.
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
  // Inline humanized load error (ErrorBanner + Details disclosure).
  let loadError = $state<HumanizedError | null>(null);
  // Normalized relevance (0..1) per entry id, for the search relevance meter.
  let searchRelevance = $state<Map<string, number>>(new Map());

  // ── Filtered entries (by entry type) ────────────────────────────────────────
  const filteredEntries = $derived.by(() => {
    if (typeFilter === "all") return entries;
    return entries.filter((e) => e.entry_type === typeFilter);
  });

  // Counters per type, for the chips.
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

  // ── Detail header (split screen : identity lives in the breadcrumb) ──────────
  const selectedNs = $derived(namespaces.find((n) => n.name === selectedNamespace));
  const detailTitle = $derived(selectedNs?.displayName ?? selectedNamespace ?? "-");
  const detailMeta = $derived(
    selectedNs?.displayName && selectedNs.displayName !== selectedNamespace
      ? selectedNamespace
      : undefined,
  );

  // ── Namespace classification ────────────────────────────────────────────────
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
        listMemoryNamespaces(),
        listAgents().catch(() => []),
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
    loadError = null;
    try {
      entries = await listMemoryEntries(selectedNamespace);
      searching = false;
      searchRelevance = new Map();
      namespaces = namespaces.map((ns) =>
        ns.name === selectedNamespace ? { ...ns, count: entries.length } : ns
      );
    } catch (e) {
      loadError = reportError(e, { surface: "inline" });
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
    loadError = null;
    try {
      const results: MemorySearchResult[] = await searchMemory(
        selectedNamespace,
        query,
      );

      entries = results.map((r) => ({
        id: r.id,
        entry_type: r.entry_type,
        key: r.entry_type,
        value: r.content,
        created_at: r.created_at,
        expires_at: null,
        score: r.score,
      }));
      // Relevance meter: prefer the backend 0..1 relevance, else normalize BM25
      // scores against the top hit so the best result fills the bar.
      const maxScore = Math.max(...results.map((r) => r.score), Number.EPSILON);
      searchRelevance = new Map(
        results.map((r) => [
          r.id,
          r.relevance ?? Math.max(0, Math.min(1, r.score / maxScore)),
        ]),
      );
      searching = true;
    } catch (e) {
      reportError(e, { surface: "toast", testid: "memory-search-error" });
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
      const deleted: boolean = await deleteMemoryEntry(selectedNamespace, entryId);
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

  /**
   * An import or a purge rewrites the namespace under the explorer: drop the
   * search view, reload the entries, and refresh the sidebar counts.
   */
  async function handleNamespaceDataChanged(): Promise<void> {
    searchQuery = "";
    searching = false;
    selectedEntry = null;
    await loadNamespaces();
    await loadEntries();
  }

  async function handleCopyEntry(entry: MemoryEntry): Promise<void> {
    try {
      await navigator.clipboard.writeText(entry.value);
      addToast($t("memory.value_copied"), "success");
    } catch {
      addToast($t("memory.copy_failed"), "error");
    }
  }

  // ── Type filter tabs (underline pattern, aligned on Projects) ───────────────
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

  // A shared namespace granted to an agent while this route is open changes the
  // sidebar without any action from the user, so the list is re-read rather
  // than left showing the state of the last visit.
  let seenMemoryChange = 0;
  $effect(() => {
    const tick = $memoryChanged;
    if (tick === seenMemoryChange) return;
    seenMemoryChange = tick;
    void loadNamespaces();
  });
</script>

<div class="flex flex-col h-full" data-testid="memory-page">
  <div class="flex flex-1 min-h-0 overflow-hidden">
    {#if !loadingMemory && namespaces.length === 0}
      <div class="flex-1 px-8 py-8">
        <EmptyState
          icon={Database}
          title={$t("memory.empty_title")}
          description={$t("memory.empty_subtitle")}
          page="memory"
        />
      </div>
    {:else}
      <SplitLayout sidebarTestid="namespace-sidebar" sidebarClass="border-border/60">
        {#snippet sidebar()}
          <!-- Sidebar: classified namespaces -->
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
            <p class="text-caption text-muted-foreground leading-snug">
              {$t("memory.profile_banner_prefix")}
              <a href="/settings/profile" class="text-primary underline-offset-2 hover:underline font-medium">
                {$t("memory.profile_banner_link")}
              </a>.
              {$t("memory.profile_banner_suffix")}
            </p>
          </div>
        {/if}

        <!-- Detail header : namespace identity + search (screen identity is the breadcrumb) -->
        <DetailHeader title={detailTitle} meta={detailMeta}>
          {#snippet leading()}
            <div class="flex h-9 w-9 items-center justify-center rounded-lg bg-muted text-muted-foreground">
              <Database size={16} />
            </div>
          {/snippet}
          {#snippet badges()}
            {#if searching}
              <span class="inline-flex items-center gap-1 text-caption text-info">
                <Search size={10} />
                {$t("memory.results_count", { values: { count: filteredEntries.length } })}
              </span>
            {/if}
          {/snippet}
          {#snippet actions()}
            <div class="w-64 max-w-[40vw]">
              <MemorySearch value={searchQuery} onsearch={handleSearch} />
            </div>
            <NamespaceDataActions
              namespace={selectedNamespace}
              onchanged={handleNamespaceDataChanged}
            />
          {/snippet}
        </DetailHeader>

        <!-- Type tabs (underline pattern, aligned on Projects) -->
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
        <div class="flex-1 overflow-y-auto" data-testid="memory-entries-list" use:listNavigation>
          {#if loadError}
            <div class="space-y-2 px-6 py-4">
              <ErrorBanner
                tone="danger"
                message={loadError.friendly_message}
                onretry={loadEntries}
                retryLabel={$t("common.retry")}
                loading={loadingMemory}
                data-testid="memory-load-error"
              />
              {#if loadError.detail}
                <Disclosure testid="memory-error-details">
                  {#snippet summary()}
                    <span class="text-caption text-muted-foreground">{$t("memory.error_details")}</span>
                  {/snippet}
                  {#snippet children()}
                    <pre class="overflow-x-auto whitespace-pre-wrap font-mono text-overline font-normal tracking-normal text-destructive">{loadError?.detail}</pre>
                  {/snippet}
                </Disclosure>
              {/if}
            </div>
          {:else if loadingMemory}
            <div class="px-6 py-4 space-y-2">
              {#each Array(5) as _}
                <LoadingShimmer width="100%" height="3rem" />
              {/each}
            </div>
          {:else if filteredEntries.length === 0}
            <div class="flex flex-col items-center justify-center gap-2 py-16 text-center">
              <div class="flex h-10 w-10 items-center justify-center rounded-full bg-muted">
                {#if searching}
                  <Search size={16} class="text-muted-foreground" />
                {:else}
                  <Database size={16} class="text-muted-foreground" />
                {/if}
              </div>
              <p class="text-body-xs text-muted-foreground">
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
                  class="text-caption text-primary hover:underline"
                >
                  {$t("memory.show_all_entries")}
                </Button>
              {/if}
            </div>
          {:else}
            {#key selectedNamespace}
              <div in:fly={contentIn()} out:fly={contentOut()}>
                {#each filteredEntries as entry (entry.id)}
                  <div animate:flip={listFlip()} in:fly={rowIn()} out:fly={rowOut()}>
                    <MemoryEntryRow
                      {entry}
                      searching={searching}
                      relevance={searching ? searchRelevance.get(entry.id) : undefined}
                      selected={selectedEntry?.id === entry.id}
                      onclick={() => (selectedEntry = entry)}
                      oncopy={() => handleCopyEntry(entry)}
                      ondelete={() => handleDelete(entry.id)}
                    />
                  </div>
                {/each}
              </div>
            {/key}
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
