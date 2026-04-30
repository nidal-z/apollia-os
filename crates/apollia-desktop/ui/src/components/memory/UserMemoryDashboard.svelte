<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type {
    UserMemoryProfileView,
    UserMemoryEntryView,
    UpdateUserMemoryRequest,
  } from "$lib/types";
  import type { UIMode } from "$lib/stores/mode";
  import { Input } from "$lib/components/ui/input";
  import { Textarea } from "$lib/components/ui/textarea";
  import { Select } from "$lib/components/ui/select";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { addToast } from "$lib/components/ui/toast/store";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import LegacyEmptyState from "../common/EmptyState.svelte";
  import MemoryRow from "./MemoryRow.svelte";
  import { Chip, StatusDot, BtnPrimary, BtnSecondary, EmptyState } from "$lib/components/operator";
  import { Brain, Plus, Search } from "lucide-svelte";

  interface Props {
    mode: UIMode;
  }

  let { mode }: Props = $props();

  type Filter = "all" | "preferences" | "habits" | "context";

  const CATEGORIES = ["preferences", "habits", "context"] as const;

  const FILTER_TONE: Record<Filter, "primary" | "secondary" | "warning" | "neutral"> = {
    all: "primary",
    preferences: "primary",
    habits: "secondary",
    context: "warning",
  };

  const FILTER_DOT: Record<Filter, string> = {
    all: "hsl(var(--muted-foreground))",
    preferences: "hsl(var(--primary))",
    habits: "hsl(var(--secondary))",
    context: "hsl(var(--warning))",
  };

  // ── State ──
  let profile = $state<UserMemoryProfileView | null>(null);
  let isLoading = $state(true);
  let searchQuery = $state("");
  let searchResults = $state<UserMemoryEntryView[] | null>(null);
  let isAddingEntry = $state(false);
  let activeFilter = $state<Filter>("all");
  let searchTimer: ReturnType<typeof setTimeout> | null = null;

  let newKey = $state("");
  let newValue = $state("");
  let newCategory = $state("preferences");
  let newKeyTouched = $state(false);

  let deleteTarget = $state<UserMemoryEntryView | null>(null);
  let isDeleting = $state(false);

  // ── Derived ──
  /** Internal keys that should not be shown to the user. */
  const HIDDEN_KEY_PREFIXES = ["onboarding_topic_", "onboarding_skipped"];

  let rawEntries = $derived(searchResults ?? profile?.entries ?? []);
  let entries = $derived(
    rawEntries.filter((e) => !HIDDEN_KEY_PREFIXES.some((p) => e.key.startsWith(p))),
  );
  let isSearching = $derived(searchResults !== null);
  let isNewKeyValid = $derived(newKey.trim().length > 0);

  let counts = $derived.by(() => {
    const c: Record<Filter, number> = { all: entries.length, preferences: 0, habits: 0, context: 0 };
    for (const e of entries) {
      if (e.category in c) c[e.category as Filter] = (c[e.category as Filter] ?? 0) + 1;
    }
    return c;
  });

  let filteredEntries = $derived(
    activeFilter === "all" ? entries : entries.filter((e) => e.category === activeFilter),
  );

  function categoryLabel(cat: string): string {
    return $t(`memory.user_memory.section_${cat}`);
  }

  // ── Data loading ──
  async function loadProfile(): Promise<void> {
    isLoading = true;
    try {
      profile = await invoke("get_user_memory_profile");
    } catch (e) {
      addToast(`${$t("memory.user_memory.toast_load_failed")}: ${e}`, "error");
    } finally {
      isLoading = false;
    }
  }

  function handleSearchInput(): void {
    if (searchTimer !== null) clearTimeout(searchTimer);
    if (searchQuery.trim() === "") {
      searchResults = null;
      return;
    }
    searchTimer = setTimeout(async () => {
      try {
        searchResults = await invoke("search_user_memory", { query: searchQuery.trim() });
      } catch (e) {
        addToast(`${$t("memory.user_memory.toast_search_failed")}: ${e}`, "error");
      }
    }, 300);
  }

  // ── CRUD handlers ──
  async function handleValidate(key: string): Promise<void> {
    try {
      await invoke("validate_user_memory", { key });
      addToast($t("memory.user_memory.toast_validated"), "success");
      await loadProfile();
      if (isSearching && searchQuery.trim() !== "") {
        searchResults = await invoke("search_user_memory", { query: searchQuery.trim() });
      }
    } catch (e) {
      addToast(`${$t("memory.user_memory.toast_validate_failed")}: ${e}`, "error");
    }
  }

  async function handleUpdate(key: string, value: string): Promise<void> {
    const existing = entries.find((e) => e.key === key);
    if (!existing) return;
    const request: UpdateUserMemoryRequest = { category: existing.category, key, value };
    try {
      await invoke("update_user_memory_entry", { request });
      addToast($t("memory.user_memory.toast_updated"), "success");
      await loadProfile();
      if (isSearching && searchQuery.trim() !== "") {
        searchResults = await invoke("search_user_memory", { query: searchQuery.trim() });
      }
    } catch (e) {
      addToast(`${$t("memory.user_memory.toast_update_failed")}: ${e}`, "error");
    }
  }

  function requestDelete(key: string): void {
    const entry = entries.find((e) => e.key === key);
    if (entry) deleteTarget = entry;
  }

  async function confirmDelete(): Promise<void> {
    if (!deleteTarget) return;
    isDeleting = true;
    try {
      await invoke("delete_user_memory_entry", { key: deleteTarget.key });
      addToast($t("memory.user_memory.toast_deleted"), "success");
      deleteTarget = null;
      await loadProfile();
      if (isSearching && searchQuery.trim() !== "") {
        searchResults = await invoke("search_user_memory", { query: searchQuery.trim() });
      }
    } catch (e) {
      addToast(`${$t("memory.user_memory.toast_delete_failed")}: ${e}`, "error");
    } finally {
      isDeleting = false;
    }
  }

  async function handleRecategorize(key: string, category: string): Promise<void> {
    const existing = entries.find((e) => e.key === key);
    if (!existing) return;
    const request: UpdateUserMemoryRequest = { category, key, value: existing.value };
    try {
      await invoke("update_user_memory_entry", { request });
      addToast($t("memory.user_memory.toast_category_updated"), "success");
      await loadProfile();
      if (isSearching && searchQuery.trim() !== "") {
        searchResults = await invoke("search_user_memory", { query: searchQuery.trim() });
      }
    } catch (e) {
      addToast(`${$t("memory.user_memory.toast_update_failed")}: ${e}`, "error");
    }
  }

  async function handleAddEntry(): Promise<void> {
    if (!isNewKeyValid) return;
    const request: UpdateUserMemoryRequest = {
      category: newCategory,
      key: newKey.trim(),
      value: newValue.trim(),
    };
    try {
      await invoke("update_user_memory_entry", { request });
      addToast($t("memory.user_memory.toast_added"), "success");
      resetAddForm();
      await loadProfile();
    } catch (e) {
      addToast(`${$t("memory.user_memory.toast_update_failed")}: ${e}`, "error");
    }
  }

  function openAddForm(): void {
    isAddingEntry = true;
    newKey = "";
    newValue = "";
    newCategory = "preferences";
    newKeyTouched = false;
  }

  function resetAddForm(): void {
    isAddingEntry = false;
    newKey = "";
    newValue = "";
    newCategory = "preferences";
    newKeyTouched = false;
  }

  onMount(loadProfile);
</script>

<div class="space-y-4" data-testid="user-memory-dashboard">
  {#if isLoading}
    <Skeleton width="100%" height="2.5rem" />
    <div class="rounded-xl border border-border/60 bg-card overflow-hidden">
      <Skeleton width="100%" height="3rem" />
      <Skeleton width="100%" height="3rem" />
      <Skeleton width="80%" height="3rem" />
    </div>
  {:else if profile && profile.stats.total === 0 && !isAddingEntry}
    <div class="rounded-xl border border-border/60 bg-card overflow-hidden">
      <LegacyEmptyState
        icon={Brain}
        title={mode === "operator"
          ? $t("memory.user_memory.empty_title_operator")
          : $t("memory.user_memory.empty_title_builder")}
        subtitle={mode === "operator"
          ? $t("memory.user_memory.empty_desc_operator")
          : $t("memory.user_memory.empty_desc_builder")}
        ctaLabel={mode === "operator"
          ? $t("memory.user_memory.add_btn_operator")
          : $t("memory.user_memory.add_btn_builder")}
        ctaAction={openAddForm}
        page="user-memory"
      />
    </div>
    {#if isAddingEntry}
      {@render addEntryForm()}
    {/if}
  {:else if profile}
    <!-- Search + Add bar -->
    <div class="flex items-center gap-2">
      <div class="relative flex-1">
        <Search size={13} class="absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground/60" />
        <input
          type="text"
          class="w-full h-9 rounded-md border border-border bg-background pl-9 pr-3 text-[12px] transition-shadow placeholder:text-muted-foreground/70 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40 focus-visible:border-primary/50"
          style="border-width: 0.5px;"
          placeholder={$t("memory.user_memory.search_placeholder")}
          bind:value={searchQuery}
          oninput={handleSearchInput}
          data-testid="search-input"
        />
      </div>
      {#if isSearching}
        <span class="text-[11px] text-muted-foreground whitespace-nowrap font-mono">
          {$t("memory.user_memory.search_results", { values: { count: entries.length } })}
        </span>
      {/if}
      <BtnPrimary onclick={openAddForm}>
        {#snippet icon()}<Plus size={12} />{/snippet}
        {mode === "operator"
          ? $t("memory.user_memory.add_btn_operator")
          : $t("memory.user_memory.add_btn_builder")}
      </BtnPrimary>
    </div>

    <!-- Filter chips row (V3 supervision filters) -->
    <div
      class="flex flex-wrap items-center gap-2"
      role="tablist"
      aria-label="Filtres de catégorie"
      data-testid="memory-filter-bar"
    >
      {#each ["all", ...CATEGORIES] as f (f)}
        {@const filter = f as Filter}
        {@const isActive = activeFilter === filter}
        <button
          type="button"
          role="tab"
          aria-selected={isActive}
          onclick={() => (activeFilter = filter)}
          class="cursor-pointer border-0 bg-transparent p-0"
          data-testid="memory-filter-{filter}"
          data-active={isActive}
        >
          <Chip
            tone={isActive ? FILTER_TONE[filter] : "neutral"}
            outline={!isActive}
            size="md"
          >
            {#snippet icon()}<StatusDot color={FILTER_DOT[filter]} />{/snippet}
            {filter === "all" ? $t("memory.user_memory.filter_all") : categoryLabel(filter)} · {counts[filter]}
          </Chip>
        </button>
      {/each}
    </div>

    <!-- Add entry form -->
    {#if isAddingEntry}
      {@render addEntryForm()}
    {/if}

    <!-- Memory table -->
    {#if filteredEntries.length === 0}
      <div class="rounded-xl border border-border/60 bg-card overflow-hidden" data-testid="memory-empty">
        <EmptyState
          title={isSearching
            ? $t("memory.no_search_results")
            : $t("memory.user_memory.empty_filter_title")}
          desc={isSearching ? "" : $t("memory.user_memory.empty_filter_desc")}
        >
          {#snippet icon()}<Brain size={22} />{/snippet}
          {#snippet action()}
            {#if !isSearching && activeFilter !== "all"}
              <BtnSecondary onclick={() => (activeFilter = "all")}>
                {$t("memory.user_memory.filter_show_all")}
              </BtnSecondary>
            {/if}
          {/snippet}
        </EmptyState>
      </div>
    {:else}
      <div class="rounded-xl border border-border/60 bg-card overflow-hidden" data-testid="memory-table">
        <!-- Column headers -->
        <div
          class="px-4 py-2.5 border-b border-border/60 flex items-center gap-2.5 text-[10.5px] uppercase tracking-[1px] font-semibold text-muted-foreground/70"
        >
          <div class="flex-[2] min-w-0">{$t("memory.user_memory.col_key")}</div>
          <div class="w-[180px]">{$t("memory.user_memory.col_category")}</div>
          <div class="w-[120px]">{$t("memory.user_memory.col_confidence")}</div>
          <div class="w-[90px] text-right">{$t("memory.user_memory.col_updated")}</div>
          <div class="w-[28px]"></div>
        </div>

        {#each filteredEntries as entry (entry.key)}
          <MemoryRow
            {entry}
            onupdate={handleUpdate}
            ondelete={requestDelete}
            onrecategorize={handleRecategorize}
            onvalidate={handleValidate}
          />
        {/each}
      </div>
    {/if}
  {/if}
</div>

<!-- Delete confirmation dialog -->
<ConfirmDialog
  open={deleteTarget !== null}
  onclose={() => { deleteTarget = null; }}
  onconfirm={confirmDelete}
  title={mode === "operator"
    ? $t("memory.user_memory.delete_title_operator")
    : $t("memory.user_memory.delete_title_builder")}
  message={deleteTarget ? `${deleteTarget.key}: ${deleteTarget.value}` : ""}
  confirmLabel={$t("memory.user_memory.delete_confirm")}
  cancelLabel={$t("memory.user_memory.delete_cancel")}
  loading={isDeleting}
  data-testid="delete-confirm"
/>

{#snippet addEntryForm()}
  <div
    class="rounded-xl border border-border/60 bg-card p-4 space-y-3"
    data-testid="add-entry-form"
  >
    <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
      <div>
        <label for="um-add-key" class="text-[11px] text-muted-foreground mb-1 block uppercase tracking-wider font-mono">
          {$t("memory.user_memory.add_key_label")}
        </label>
        <Input
          id="um-add-key"
          bind:value={newKey}
          placeholder={mode === "operator"
            ? $t("memory.user_memory.add_placeholder_key_operator")
            : $t("memory.user_memory.add_placeholder_key_builder")}
          oninput={() => { newKeyTouched = true; }}
        />
        {#if newKeyTouched && !isNewKeyValid}
          <p class="text-[10px] text-danger-a11y mt-1">{$t("memory.user_memory.add_key_required")}</p>
        {/if}
      </div>
      <div>
        <label for="um-add-category" class="text-[11px] text-muted-foreground mb-1 block uppercase tracking-wider font-mono">
          {$t("memory.user_memory.add_category_label")}
        </label>
        <Select id="um-add-category" bind:value={newCategory}>
          {#each CATEGORIES as cat}
            <option value={cat}>{categoryLabel(cat)}</option>
          {/each}
        </Select>
      </div>
    </div>
    <div>
      <label for="um-add-value" class="text-[11px] text-muted-foreground mb-1 block uppercase tracking-wider font-mono">
        {$t("memory.user_memory.add_value_label")}
      </label>
      <Textarea
        id="um-add-value"
        bind:value={newValue}
        placeholder={mode === "operator"
          ? $t("memory.user_memory.add_placeholder_value_operator")
          : $t("memory.user_memory.add_placeholder_value_builder")}
      />
    </div>
    <div class="flex gap-2 justify-end">
      <BtnSecondary onclick={resetAddForm}>
        {$t("memory.user_memory.add_cancel")}
      </BtnSecondary>
      <BtnPrimary onclick={handleAddEntry} disabled={!isNewKeyValid}>
        {$t("memory.user_memory.add_save")}
      </BtnPrimary>
    </div>
  </div>
{/snippet}
