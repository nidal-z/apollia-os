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
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Textarea } from "$lib/components/ui/textarea";
  import { Select } from "$lib/components/ui/select";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { addToast } from "$lib/components/ui/toast/store";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import EmptyState from "../common/EmptyState.svelte";
  import UserMemoryEntryCard from "./UserMemoryEntryCard.svelte";
  import { formatRelativeTime } from "$lib/utils";
  import { Brain, Plus, Search } from "lucide-svelte";

  interface Props {
    mode: UIMode;
  }

  let { mode }: Props = $props();

  // ── State ──
  let profile = $state<UserMemoryProfileView | null>(null);
  let isLoading = $state(true);
  let searchQuery = $state("");
  let searchResults = $state<UserMemoryEntryView[] | null>(null);
  let isAddingEntry = $state(false);
  let searchTimer: ReturnType<typeof setTimeout> | null = null;

  // Add form state
  let newKey = $state("");
  let newValue = $state("");
  let newCategory = $state("preferences");
  let newKeyTouched = $state(false);

  // Delete confirm state
  let deleteTarget = $state<UserMemoryEntryView | null>(null);
  let isDeleting = $state(false);

  const CATEGORY_COLORS: Record<string, string> = {
    preferences: "#3435f5",
    habits: "#7c5fd6",
    context: "#f59e0b",
  };

  const SOURCE_COLORS: Record<string, string> = {
    onboarding: "#10b981",
    chat_inference: "#3435f5",
    user_explicit: "#7c5fd6",
    agent_observation: "#f59e0b",
  };

  const CATEGORIES = ["preferences", "habits", "context"] as const;

  // ── Derived ──
  let entries = $derived(searchResults ?? profile?.entries ?? []);
  let isSearching = $derived(searchResults !== null);

  let groupedEntries = $derived.by(() => {
    const groups: Record<string, UserMemoryEntryView[]> = {
      preferences: [],
      habits: [],
      context: [],
    };
    for (const entry of entries) {
      const cat = entry.category;
      if (cat in groups) {
        groups[cat].push(entry);
      }
    }
    return groups;
  });

  let isNewKeyValid = $derived(newKey.trim().length > 0);

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
    if (searchTimer !== null) {
      clearTimeout(searchTimer);
    }

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
  async function handleUpdate(key: string, value: string): Promise<void> {
    const existing = entries.find((e) => e.key === key);
    if (!existing) return;

    const request: UpdateUserMemoryRequest = {
      category: existing.category,
      key,
      value,
    };

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
    if (entry) {
      deleteTarget = entry;
    }
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

    const request: UpdateUserMemoryRequest = {
      category,
      key,
      value: existing.value,
    };

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

  // ── Stats helpers ──
  function categoryBarWidth(count: number, total: number): string {
    if (total === 0) return "0%";
    return `${(count / total) * 100}%`;
  }

  function sourceArcData(stats: UserMemoryProfileView["stats"]): { key: string; count: number; color: string }[] {
    return [
      { key: "onboarding", count: stats.by_source.onboarding, color: SOURCE_COLORS.onboarding },
      { key: "chat_inference", count: stats.by_source.chat_inference, color: SOURCE_COLORS.chat_inference },
      { key: "user_explicit", count: stats.by_source.user_explicit, color: SOURCE_COLORS.user_explicit },
      { key: "agent_observation", count: stats.by_source.agent_observation, color: SOURCE_COLORS.agent_observation },
    ].filter((s) => s.count > 0);
  }

  function sectionLabel(cat: string): string {
    const key = `memory.user_memory.section_${cat}`;
    return $t(key);
  }

  onMount(loadProfile);
</script>

<div class="space-y-6" data-testid="user-memory-dashboard">
  {#if isLoading}
    <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
      <Skeleton width="100%" height="5rem" />
      <Skeleton width="100%" height="5rem" />
      <Skeleton width="100%" height="5rem" />
      <Skeleton width="100%" height="5rem" />
    </div>
    <Skeleton width="100%" height="2.5rem" />
    <div class="space-y-2">
      <Skeleton width="100%" height="4rem" />
      <Skeleton width="100%" height="4rem" />
      <Skeleton width="80%" height="4rem" />
    </div>
  {:else if profile && profile.stats.total === 0 && !isAddingEntry}
    <!-- Empty state -->
    <EmptyState
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

    <!-- Add form over empty state -->
    {#if isAddingEntry}
      {@render addEntryForm()}
    {/if}
  {:else if profile}
    <!-- Stats cards -->
    <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
      <!-- Total -->
      <div class="rounded-xl glass-card glass-border p-4" data-testid="stat-card-total">
        <p class="text-xs text-muted-foreground/60 uppercase tracking-wider">{$t("memory.user_memory.stat_total")}</p>
        <p class="text-3xl font-bold mt-1">{profile.stats.total}</p>
        <p class="text-xs text-muted-foreground/60 mt-0.5">{$t("memory.user_memory.stat_total_sub")}</p>
      </div>

      <!-- Categories stacked bar -->
      <div class="rounded-xl glass-card glass-border p-4" data-testid="stat-card-categories">
        <p class="text-xs text-muted-foreground/60 uppercase tracking-wider">{$t("memory.user_memory.stat_categories")}</p>
        <div class="mt-3 flex h-3 w-full rounded-full overflow-hidden bg-muted/30">
          {#each CATEGORIES as cat}
            {@const count = profile.stats.by_category[cat]}
            {#if count > 0}
              <div
                class="h-full transition-all relative group/seg"
                style="width: {categoryBarWidth(count, profile.stats.total)}; background-color: {CATEGORY_COLORS[cat]}"
                title="{cat}: {count}"
              >
                <span class="absolute -top-7 left-1/2 -translate-x-1/2 hidden group-hover/seg:block text-[10px] bg-background border border-border rounded px-1.5 py-0.5 whitespace-nowrap shadow-sm">
                  {cat}: {count}
                </span>
              </div>
            {/if}
          {/each}
        </div>
        <div class="flex gap-3 mt-2">
          {#each CATEGORIES as cat}
            <div class="flex items-center gap-1">
              <span class="w-2 h-2 rounded-full" style="background-color: {CATEGORY_COLORS[cat]}"></span>
              <span class="text-[10px] text-muted-foreground">{cat}</span>
            </div>
          {/each}
        </div>
      </div>

      <!-- Sources donut -->
      <div class="rounded-xl glass-card glass-border p-4" data-testid="stat-card-sources">
        <p class="text-xs text-muted-foreground/60 uppercase tracking-wider">{$t("memory.user_memory.stat_sources")}</p>
        <div class="mt-2 flex items-center gap-4">
          <!-- Mini donut via SVG -->
          <svg width="48" height="48" viewBox="0 0 48 48" class="shrink-0">
            {#if profile.stats.total === 0}
              <circle cx="24" cy="24" r="18" fill="none" stroke="currentColor" stroke-width="6" class="text-muted/30" />
            {:else}
              {@const arcs = sourceArcData(profile.stats)}
              {@const circumference = 2 * Math.PI * 18}
              {#each arcs as arc, i}
                {@const total = profile?.stats.total ?? 1}
                {@const offset = arcs.slice(0, i).reduce((sum, a) => sum + (a.count / total) * circumference, 0)}
                <circle
                  cx="24" cy="24" r="18"
                  fill="none"
                  stroke={arc.color}
                  stroke-width="6"
                  stroke-dasharray="{(arc.count / total) * circumference} {circumference}"
                  stroke-dashoffset={-offset}
                  transform="rotate(-90 24 24)"
                />
              {/each}
            {/if}
          </svg>
          <div class="flex flex-col gap-1">
            {#each sourceArcData(profile.stats) as src}
              <div class="flex items-center gap-1.5">
                <span class="w-2 h-2 rounded-full" style="background-color: {src.color}"></span>
                <span class="text-[10px] text-muted-foreground">{src.key.replace("_", " ")}: {src.count}</span>
              </div>
            {/each}
          </div>
        </div>
      </div>

      <!-- Last updated -->
      <div class="rounded-xl glass-card glass-border p-4" data-testid="stat-card-updated">
        <p class="text-xs text-muted-foreground/60 uppercase tracking-wider">{$t("memory.user_memory.stat_updated")}</p>
        <p class="text-xl font-semibold mt-1">
          {profile.stats.last_updated_at
            ? formatRelativeTime(profile.stats.last_updated_at)
            : $t("memory.user_memory.stat_never")}
        </p>
      </div>
    </div>

    <!-- Search bar + Add button -->
    <div class="flex items-center gap-3">
      <div class="relative flex-1">
        <Search size={14} class="absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground/50" />
        <input
          type="text"
          class="w-full h-10 rounded-md border border-border bg-background pl-9 pr-3 text-sm ring-offset-background transition-shadow duration-150 placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40 focus-visible:border-primary/50"
          placeholder={$t("memory.user_memory.search_placeholder")}
          bind:value={searchQuery}
          oninput={handleSearchInput}
          data-testid="search-input"
        />
      </div>
      {#if isSearching}
        <span class="text-xs text-muted-foreground whitespace-nowrap">
          {$t("memory.user_memory.search_results", { values: { count: entries.length } })}
        </span>
      {/if}
      <Button size="sm" onclick={openAddForm} data-testid="add-entry-btn">
        <Plus size={14} class="mr-1" />
        {mode === "operator"
          ? $t("memory.user_memory.add_btn_operator")
          : $t("memory.user_memory.add_btn_builder")}
      </Button>
    </div>

    <!-- Add entry form -->
    {#if isAddingEntry}
      {@render addEntryForm()}
    {/if}

    <!-- Grouped entries -->
    {#each CATEGORIES as cat}
      {@const catEntries = groupedEntries[cat]}
      {#if catEntries.length > 0}
        <div>
          <div class="flex items-center gap-2 mb-3">
            <span
              class="inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-medium text-white"
              style="background-color: {CATEGORY_COLORS[cat]}"
            >
              {sectionLabel(cat)}
            </span>
            <span class="text-xs text-muted-foreground/60">{catEntries.length}</span>
          </div>
          <div class="space-y-2">
            {#each catEntries as entry (entry.key)}
              <UserMemoryEntryCard
                {entry}
                onupdate={handleUpdate}
                ondelete={requestDelete}
                onrecategorize={handleRecategorize}
              />
            {/each}
          </div>
        </div>
      {/if}
    {/each}

    <!-- No results for search -->
    {#if isSearching && entries.length === 0}
      <p class="text-center text-sm text-muted-foreground/60 py-8">
        {$t("memory.no_search_results")}
      </p>
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
  <div class="rounded-xl glass-card glass-border p-4 space-y-3" data-testid="add-entry-form">
    <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
      <div>
        <label for="um-add-key" class="text-xs text-muted-foreground mb-1 block">{$t("memory.user_memory.add_key_label")}</label>
        <Input
          id="um-add-key"
          bind:value={newKey}
          placeholder={mode === "operator"
            ? $t("memory.user_memory.add_placeholder_key_operator")
            : $t("memory.user_memory.add_placeholder_key_builder")}
          oninput={() => { newKeyTouched = true; }}
        />
        {#if newKeyTouched && !isNewKeyValid}
          <p class="text-[10px] text-red-500 mt-1">{$t("memory.user_memory.add_key_required")}</p>
        {/if}
      </div>
      <div>
        <label for="um-add-category" class="text-xs text-muted-foreground mb-1 block">{$t("memory.user_memory.add_category_label")}</label>
        <Select id="um-add-category" bind:value={newCategory}>
          {#each CATEGORIES as cat}
            <option value={cat}>{cat}</option>
          {/each}
        </Select>
      </div>
    </div>
    <div>
      <label for="um-add-value" class="text-xs text-muted-foreground mb-1 block">{$t("memory.user_memory.add_value_label")}</label>
      <Textarea
        id="um-add-value"
        bind:value={newValue}
        placeholder={mode === "operator"
          ? $t("memory.user_memory.add_placeholder_value_operator")
          : $t("memory.user_memory.add_placeholder_value_builder")}
      />
    </div>
    <div class="flex gap-2 justify-end">
      <Button variant="outline" size="sm" onclick={resetAddForm}>
        {$t("memory.user_memory.add_cancel")}
      </Button>
      <Button size="sm" onclick={handleAddEntry} disabled={!isNewKeyValid}>
        {$t("memory.user_memory.add_save")}
      </Button>
    </div>
  </div>
{/snippet}
