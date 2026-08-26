<script lang="ts">
  import { Database, Search, Layers, Bot, FolderOpen, User, HelpCircle } from "lucide-svelte";
  import { t } from "svelte-i18n";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { Input } from "$lib/components/ui/input";
  import { Button } from "$lib/components/ui/button";
  import { SidebarHeader, ListRow } from "$lib/components/operator";

  /**
   * Semantic category of one memory namespace.
   * - `user`: shared user profile (`__user__`).
   * - `agent`: namespace of an installed agent (matches `manifest.memory_namespace`).
   * - `project`: namespace scoped to a project (format `{project_id}:{namespace}`).
   * - `other`: unclassified (legacy, uninstalled agents, manual).
   */
  export type NamespaceCategory = "user" | "agent" | "project" | "other";

  export interface NamespaceItem {
    name: string;
    /** Total entries (every type). Optional - displayed when defined. */
    count?: number;
    /** Category for the filter - computed by the caller. */
    category: NamespaceCategory;
    /** Optional human label (agent name, project name) shown in place of the raw name. */
    displayName?: string;
  }

  interface Props {
    namespaces: NamespaceItem[];
    selected: string;
    /** Loading state - shows a placeholder. */
    loading?: boolean;
    onselect: (namespace: string) => void;
  }

  let { namespaces, selected, loading = false, onselect }: Props = $props();

  // ── Local state: category filter plus text filter ───────────────────────────
  type CategoryFilter = "all" | NamespaceCategory;
  let categoryFilter = $state<CategoryFilter>("all");
  let filterQuery = $state("");

  // Counters per category
  const counts = $derived.by(() => {
    const c = { all: namespaces.length, user: 0, agent: 0, project: 0, other: 0 };
    for (const ns of namespaces) c[ns.category]++;
    return c;
  });

  // Filtered list (category plus text search)
  const filteredNamespaces = $derived.by(() => {
    let list = categoryFilter === "all"
      ? namespaces
      : namespaces.filter((ns) => ns.category === categoryFilter);
    if (filterQuery.trim() !== "") {
      const q = filterQuery.toLowerCase();
      list = list.filter(
        (ns) =>
          ns.name.toLowerCase().includes(q) ||
          (ns.displayName && ns.displayName.toLowerCase().includes(q)),
      );
    }
    return list;
  });

  // Grouping: the "all" filter groups by category, otherwise a flat list.
  const grouped = $derived.by(() => {
    if (categoryFilter !== "all") {
      return [{ category: categoryFilter, items: filteredNamespaces }];
    }
    const order: NamespaceCategory[] = ["user", "agent", "project", "other"];
    const groups: { category: NamespaceCategory; items: NamespaceItem[] }[] = [];
    for (const cat of order) {
      const items = filteredNamespaces.filter((ns) => ns.category === cat);
      if (items.length > 0) groups.push({ category: cat, items });
    }
    return groups;
  });

  // ── Definition of the category filter chips ─────────────────────────────────
  type FilterChipDef = { key: CategoryFilter; labelKey: string; Icon: typeof Database };
  const filterChips: FilterChipDef[] = [
    { key: "all", labelKey: "memory.namespaces.cat_all", Icon: Layers },
    { key: "agent", labelKey: "memory.namespaces.cat_agent", Icon: Bot },
    { key: "project", labelKey: "memory.namespaces.cat_project", Icon: FolderOpen },
    { key: "user", labelKey: "memory.namespaces.cat_user", Icon: User },
    { key: "other", labelKey: "memory.namespaces.cat_other", Icon: HelpCircle },
  ];

  function categoryIcon(cat: NamespaceCategory) {
    return cat === "user" ? User : cat === "agent" ? Bot : cat === "project" ? FolderOpen : Database;
  }
</script>

<!-- Shell (width, border, bg) comes from the parent SplitLayout; this is content-only. -->
<div class="flex h-full flex-col">
  <SidebarHeader
    title={$t("memory.namespaces.title")}
    count={namespaces.length}
    class="border-b border-border/40"
  >
    {#snippet search()}
      <div class="relative">
        <Search
          size={12}
          class="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground"
        />
        <Input
          type="search"
          bind:value={filterQuery}
          placeholder={$t("memory.namespaces.filter_placeholder")}
          aria-label={$t("memory.namespaces.filter_placeholder")}
          class="h-8 w-full rounded-md border border-border bg-background pl-7 pr-2 text-caption text-foreground placeholder:text-muted-foreground/70 focus:outline-none focus:ring-1 focus:ring-primary/40"
          data-testid="namespace-sidebar-filter"
        />
      </div>
    {/snippet}
    {#snippet filters()}
      <div
        class="inline-flex w-full items-center gap-0.5 overflow-x-auto rounded-md border border-border/40 bg-muted/40 p-0.5"
        role="tablist"
        aria-label={$t("memory.namespaces.category_filter")}
      >
        {#each filterChips as chip}
          {@const isActive = categoryFilter === chip.key}
          {@const count = counts[chip.key]}
          {@const Icon = chip.Icon}
          <Button
            variant="ghost"
            size="sm"
            type="button"
            role="tab"
            aria-selected={isActive}
            onclick={() => (categoryFilter = chip.key)}
            disabled={count === 0 && chip.key !== "all"}
            class="inline-flex shrink-0 items-center gap-1 rounded px-1.5 py-0.5 text-overline font-medium tracking-tight transition-colors disabled:cursor-not-allowed disabled:opacity-40 {isActive
              ? 'bg-background text-foreground shadow-elev-1'
              : 'text-muted-foreground hover:bg-background/50 hover:text-foreground'}"
            data-testid="namespace-sidebar-cat-{chip.key}"
            title={$t(chip.labelKey)}
          >
            <Icon size={10} />
            <span class="hidden sm:inline">{$t(chip.labelKey)}</span>
            <span class="tabular-nums {isActive ? 'text-muted-foreground' : 'text-muted-foreground/60'}">
              {count}
            </span>
          </Button>
        {/each}
      </div>
    {/snippet}
  </SidebarHeader>

  <nav class="flex-1 overflow-y-auto px-2" aria-label={$t("memory.namespaces.title")}>
    {#if loading}
      <div class="space-y-1.5 px-2 py-3">
        {#each Array(4) as _}
          <Skeleton class="h-8 rounded-md bg-muted/40" />
        {/each}
      </div>
    {:else if filteredNamespaces.length === 0}
      <div class="px-3 py-6 text-center text-caption text-muted-foreground/70">
        {filterQuery
          ? $t("memory.namespaces.no_match")
          : categoryFilter !== "all"
            ? $t("memory.namespaces.empty_category")
            : $t("memory.namespaces.empty")}
      </div>
    {:else}
      {#each grouped as group}
        {#if categoryFilter === "all" && grouped.length > 1}
          <!-- Group header (only when several categories are visible) -->
          {@const GroupIcon = categoryIcon(group.category)}
          <div class="mt-1 flex items-center gap-1.5 px-3 py-1.5 text-overline uppercase text-muted-foreground/70 first:mt-0">
            <GroupIcon size={9} />
            <span>{$t(`memory.namespaces.cat_${group.category}_header`)}</span>
            <span class="ml-auto tabular-nums">{group.items.length}</span>
          </div>
        {/if}
        {#each group.items as ns (ns.name)}
          {@const isActive = selected === ns.name}
          {@const ItemIcon = categoryIcon(ns.category)}
          <ListRow
            variant="nav"
            state={isActive ? "active" : "default"}
            align="center"
            onclick={() => onselect(ns.name)}
            data-testid="namespace-sidebar-item-{ns.name}"
          >
            <div
              class="inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-md {isActive
                ? 'bg-gradient-primary text-primary-foreground shadow-primary-sm'
                : 'bg-muted text-muted-foreground'}"
            >
              <ItemIcon size={10} />
            </div>
            <div class="min-w-0 flex-1">
              <div
                class="truncate text-body-xs {isActive ? 'text-primary' : 'text-foreground'}"
                style:font-weight={isActive ? 600 : 500}
                title={ns.name}
              >
                {#if ns.displayName}
                  {ns.displayName}
                {:else}
                  <span class="font-mono">{ns.name}</span>
                {/if}
              </div>
              {#if ns.displayName && ns.displayName !== ns.name}
                <div class="truncate font-mono text-overline font-normal tracking-normal text-muted-foreground/60" title={ns.name}>
                  {ns.name}
                </div>
              {/if}
            </div>
            {#if ns.count !== undefined}
              <span class="text-caption tabular-nums {isActive ? 'text-primary/80' : 'text-muted-foreground/70'}">
                {ns.count}
              </span>
            {/if}
          </ListRow>
        {/each}
      {/each}
    {/if}
  </nav>
</div>
