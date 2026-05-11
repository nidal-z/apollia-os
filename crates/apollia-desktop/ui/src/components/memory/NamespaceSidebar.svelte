<script lang="ts">
  import { Database, Search, Layers, Bot, FolderOpen, User, HelpCircle } from "lucide-svelte";
  import { t } from "svelte-i18n";

  /**
   * Catégorie sémantique d'un namespace mémoire.
   * - `user` : profil utilisateur partagé (`__user__`).
   * - `agent` : namespace d'un agent installé (correspond à `manifest.memory_namespace`).
   * - `project` : namespace scopé à un projet (format `{project_id}:{namespace}`).
   * - `other` : non classifié (legacy, agents désinstallés, manuel).
   */
  export type NamespaceCategory = "user" | "agent" | "project" | "other";

  export interface NamespaceItem {
    name: string;
    /** Total entries (toutes types confondus). Optionnel — affiché si défini. */
    count?: number;
    /** Catégorie pour le filtre — calculée par le caller. */
    category: NamespaceCategory;
    /** Label humain optionnel (ex: nom de l'agent, nom du projet) à afficher en lieu et place du nom raw. */
    displayName?: string;
  }

  interface Props {
    namespaces: NamespaceItem[];
    selected: string;
    /** Loading state — affiche un placeholder. */
    loading?: boolean;
    onselect: (namespace: string) => void;
  }

  let { namespaces, selected, loading = false, onselect }: Props = $props();

  // ── État local : filtre par catégorie + filtre texte ─────────────────────────
  type CategoryFilter = "all" | NamespaceCategory;
  let categoryFilter = $state<CategoryFilter>("all");
  let filterQuery = $state("");

  // Compteurs par catégorie
  const counts = $derived.by(() => {
    const c = { all: namespaces.length, user: 0, agent: 0, project: 0, other: 0 };
    for (const ns of namespaces) {
      c[ns.category]++;
    }
    return c;
  });

  // Liste filtrée (catégorie + recherche texte)
  const filteredNamespaces = $derived.by(() => {
    let list = categoryFilter === "all"
      ? namespaces
      : namespaces.filter((ns) => ns.category === categoryFilter);
    if (filterQuery.trim() !== "") {
      const q = filterQuery.toLowerCase();
      list = list.filter(
        (ns) =>
          ns.name.toLowerCase().includes(q) ||
          (ns.displayName && ns.displayName.toLowerCase().includes(q))
      );
    }
    return list;
  });

  // Groupage : si filtre = "all", on regroupe visuellement par catégorie.
  // Sinon (filtre actif), liste plate sans header.
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

  // ── Définition des chips de filtre catégorie ─────────────────────────────────
  type FilterChipDef = {
    key: CategoryFilter;
    labelKey: string;
    defaultLabel: string;
    Icon: typeof Database;
  };
  const filterChips: FilterChipDef[] = [
    { key: "all", labelKey: "memory.namespaces.cat_all", defaultLabel: "Tous", Icon: Layers },
    { key: "agent", labelKey: "memory.namespaces.cat_agent", defaultLabel: "Agents", Icon: Bot },
    { key: "project", labelKey: "memory.namespaces.cat_project", defaultLabel: "Projets", Icon: FolderOpen },
    { key: "user", labelKey: "memory.namespaces.cat_user", defaultLabel: "Profil", Icon: User },
    { key: "other", labelKey: "memory.namespaces.cat_other", defaultLabel: "Autres", Icon: HelpCircle },
  ];

  // Label humain pour les headers de groupe
  function categoryLabel(cat: NamespaceCategory): string {
    const def = {
      user: "Profil utilisateur",
      agent: "Agents",
      project: "Projets",
      other: "Autres",
    }[cat];
    return $t(`memory.namespaces.cat_${cat}_header`, { default: def });
  }

  function categoryIcon(cat: NamespaceCategory) {
    return cat === "user" ? User : cat === "agent" ? Bot : cat === "project" ? FolderOpen : Database;
  }
</script>

<aside
  class="flex h-full w-[280px] shrink-0 flex-col border-r border-border/60 bg-surface-1/40"
  data-testid="namespace-sidebar"
>
  <header class="px-4 pt-4 pb-3 border-b border-border/40 space-y-3">
    <div class="flex items-center gap-2">
      <Layers size={14} class="text-muted-foreground" />
      <h2 class="text-[12.5px] font-semibold text-foreground tracking-tight">
        {$t('memory.namespaces.title', { default: 'Namespaces' })}
      </h2>
      <span class="ml-auto text-[10.5px] text-muted-foreground tabular-nums">
        {namespaces.length}
      </span>
    </div>

    <!-- Search -->
    <div class="relative">
      <Search
        size={12}
        class="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground"
      />
      <input
        type="search"
        bind:value={filterQuery}
        placeholder={$t('memory.namespaces.filter_placeholder', { default: 'Filtrer…' })}
        class="h-8 w-full rounded-md border border-border bg-background pl-7 pr-2 text-[11.5px] text-foreground placeholder:text-muted-foreground/70 focus:outline-none focus:ring-1 focus:ring-primary/40"
        data-testid="namespace-sidebar-filter"
      />
    </div>

    <!-- Category filter — segmented control horizontal scroll -->
    <div
      class="inline-flex w-full items-center gap-0.5 p-0.5 rounded-md bg-muted/40 border border-border/40 overflow-x-auto"
      role="tablist"
      aria-label={$t('memory.namespaces.category_filter', { default: 'Filtrer par catégorie' })}
    >
      {#each filterChips as chip}
        {@const isActive = categoryFilter === chip.key}
        {@const count = counts[chip.key]}
        {@const Icon = chip.Icon}
        <button
          type="button"
          role="tab"
          aria-selected={isActive}
          onclick={() => (categoryFilter = chip.key)}
          disabled={count === 0 && chip.key !== "all"}
          class="inline-flex shrink-0 items-center gap-1 px-1.5 py-0.5 rounded text-[10.5px] font-medium tracking-tight transition-colors disabled:opacity-40 disabled:cursor-not-allowed {isActive
            ? 'bg-background text-foreground shadow-sm'
            : 'text-muted-foreground hover:text-foreground hover:bg-background/50'}"
          data-testid="namespace-sidebar-cat-{chip.key}"
          title={$t(chip.labelKey, { default: chip.defaultLabel })}
        >
          <Icon size={10} />
          <span class="hidden sm:inline">{$t(chip.labelKey, { default: chip.defaultLabel })}</span>
          <span class="tabular-nums {isActive ? 'text-muted-foreground' : 'text-muted-foreground/60'}">
            {count}
          </span>
        </button>
      {/each}
    </div>
  </header>

  <nav class="flex-1 overflow-y-auto" aria-label={$t('memory.namespaces.title', { default: 'Namespaces' })}>
    {#if loading}
      <div class="px-2 py-3 space-y-1.5">
        {#each Array(4) as _}
          <div class="h-8 rounded-md bg-muted/40 animate-pulse"></div>
        {/each}
      </div>
    {:else if filteredNamespaces.length === 0}
      <div class="px-3 py-6 text-center text-[11.5px] text-muted-foreground/70">
        {filterQuery
          ? $t('memory.namespaces.no_match', { default: 'Aucun namespace ne correspond.' })
          : categoryFilter !== "all"
            ? $t('memory.namespaces.empty_category', { default: 'Aucun namespace dans cette catégorie.' })
            : $t('memory.namespaces.empty', { default: 'Aucun namespace.' })}
      </div>
    {:else}
      {#each grouped as group}
        {#if categoryFilter === "all" && grouped.length > 1}
          <!-- Group header (uniquement si plusieurs catégories visibles) -->
          {@const GroupIcon = categoryIcon(group.category)}
          <div class="px-3 py-1.5 mt-1 first:mt-0 flex items-center gap-1.5 text-[9.5px] uppercase tracking-[1px] font-semibold text-muted-foreground/70">
            <GroupIcon size={9} />
            <span>{categoryLabel(group.category)}</span>
            <span class="ml-auto tabular-nums">{group.items.length}</span>
          </div>
        {/if}
        {#each group.items as ns (ns.name)}
          {@const isActive = selected === ns.name}
          {@const ItemIcon = categoryIcon(ns.category)}
          <button
            type="button"
            class="group flex w-full items-center gap-2 px-3 py-2 text-left transition-colors {isActive
              ? 'bg-primary/10 text-primary'
              : 'text-foreground hover:bg-muted/40'}"
            onclick={() => onselect(ns.name)}
            data-testid="namespace-sidebar-item-{ns.name}"
          >
            <div
              class="w-[20px] h-[20px] rounded-md inline-flex items-center justify-center {isActive
                ? 'bg-primary text-white'
                : 'bg-muted text-muted-foreground'}"
            >
              <ItemIcon size={10} />
            </div>
            <div class="flex-1 min-w-0">
              <div
                class="text-[12px] truncate"
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
                <div class="text-[10px] font-mono text-muted-foreground/60 truncate" title={ns.name}>
                  {ns.name}
                </div>
              {/if}
            </div>
            {#if ns.count !== undefined}
              <span class="text-[10.5px] tabular-nums {isActive ? 'text-primary/80' : 'text-muted-foreground/70'}">
                {ns.count}
              </span>
            {/if}
          </button>
        {/each}
      {/each}
    {/if}
  </nav>
</aside>
