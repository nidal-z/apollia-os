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
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { addToast } from "$lib/components/ui/toast/store";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import UserMemoryEntryCard from "../../components/memory/UserMemoryEntryCard.svelte";
  import EmptyState from "../../components/common/EmptyState.svelte";
  import { Brain } from "lucide-svelte";

  interface Props {
    mode: UIMode;
  }

  let { mode }: Props = $props();

  // ── State ──
  let profile = $state<UserMemoryProfileView | null>(null);
  let isLoading = $state(true);

  // Accordion state: which categories are expanded
  let expandedCategories = $state<Record<string, boolean>>({
    preferences: true,
    habits: true,
    context: true,
  });

  // Delete confirm state
  let deleteTarget = $state<UserMemoryEntryView | null>(null);
  let isDeleting = $state(false);

  const CATEGORY_LABELS: Record<string, { en: string; fr: string }> = {
    preferences: { en: "Preferences", fr: "Préférences" },
    habits: { en: "Habits", fr: "Habitudes" },
    context: { en: "Context", fr: "Contexte" },
  };

  const CATEGORY_COLORS: Record<string, string> = {
    preferences: "#3435f5",
    habits: "#7c5fd6",
    context: "#f59e0b",
  };

  const SOURCE_BADGE_COLORS: Record<string, { bg: string; text: string }> = {
    onboarding: { bg: "bg-[#3435f5]/15", text: "text-[#3435f5]" },
    chat_inference: { bg: "bg-amber-500/15", text: "text-amber-600" },
    user_explicit: { bg: "bg-emerald-500/15", text: "text-emerald-600" },
    agent_observation: { bg: "bg-purple-500/15", text: "text-purple-600" },
  };

  const CATEGORIES = ["preferences", "habits", "context"] as const;

  // ── Derived ──
  let entries = $derived(profile?.entries ?? []);

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

  // ── CRUD handlers ──
  async function handleValidate(key: string): Promise<void> {
    try {
      await invoke("validate_user_memory", { key });
      addToast($t("memory.user_memory.toast_validated"), "success");
      await loadProfile();
    } catch (e) {
      addToast(`${$t("memory.user_memory.toast_validate_failed")}: ${e}`, "error");
    }
  }

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
    } catch (e) {
      addToast(`${$t("memory.user_memory.toast_update_failed")}: ${e}`, "error");
    }
  }

  function toggleCategory(cat: string): void {
    expandedCategories[cat] = !expandedCategories[cat];
  }

  function sourceBadgeLabel(source: string): string {
    const key = `settings.memories_source_${source}`;
    return $t(key);
  }

  onMount(loadProfile);
</script>

<div class="space-y-6" data-testid="user-memories-feedback">
  <p class="text-sm text-muted-foreground">
    {$t("settings.memories_subtitle")}
  </p>

  {#if isLoading}
    <div class="space-y-4">
      <Skeleton width="100%" height="4rem" />
      <Skeleton width="100%" height="4rem" />
      <Skeleton width="100%" height="4rem" />
    </div>
  {:else if !profile || entries.length === 0}
    <EmptyState
      icon={Brain}
      title={mode === "operator"
        ? $t("memory.user_memory.empty_title_operator")
        : $t("memory.user_memory.empty_title_builder")}
      subtitle={mode === "operator"
        ? $t("memory.user_memory.empty_desc_operator")
        : $t("memory.user_memory.empty_desc_builder")}
      page="user-memories"
    />
  {:else}
    <!-- Source legend -->
    <div class="flex flex-wrap gap-3" data-testid="source-legend">
      {#each Object.entries(SOURCE_BADGE_COLORS) as [source, colors]}
        {@const count = entries.filter((e) => e.source === source).length}
        {#if count > 0}
          <div class="flex items-center gap-1.5">
            <span class="inline-flex items-center rounded-full px-2 py-0.5 text-[10px] font-medium {colors.bg} {colors.text}">
              {sourceBadgeLabel(source)}
            </span>
            <span class="text-[10px] text-muted-foreground/60">{count}</span>
          </div>
        {/if}
      {/each}
    </div>

    <!-- Categories accordion -->
    {#each CATEGORIES as cat}
      {@const catEntries = groupedEntries[cat]}
      {#if catEntries.length > 0}
        <div class="rounded-xl glass-card glass-border overflow-hidden" data-testid="category-{cat}">
          <!-- Accordion header -->
          <button
            class="flex w-full items-center justify-between px-4 py-3 transition-colors hover:bg-muted/30"
            onclick={() => toggleCategory(cat)}
            data-testid="toggle-category-{cat}"
          >
            <div class="flex items-center gap-2">
              <span
                class="inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-medium text-white"
                style="background-color: {CATEGORY_COLORS[cat]}"
              >
                {$t(`memory.user_memory.section_${cat}`)}
              </span>
              <span class="text-xs text-muted-foreground/60">{catEntries.length}</span>
            </div>
            <svg
              class="h-4 w-4 text-muted-foreground transition-transform duration-200 {expandedCategories[cat] ? 'rotate-180' : ''}"
              fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"
            >
              <path stroke-linecap="round" stroke-linejoin="round" d="M19 9l-7 7-7-7" />
            </svg>
          </button>

          <!-- Accordion body -->
          {#if expandedCategories[cat]}
            <div class="border-t border-border/30 px-4 py-3 space-y-2">
              {#each catEntries as entry (entry.key)}
                <UserMemoryEntryCard
                  {entry}
                  onupdate={handleUpdate}
                  ondelete={requestDelete}
                  onrecategorize={handleRecategorize}
                  onvalidate={handleValidate}
                />
              {/each}
            </div>
          {/if}
        </div>
      {/if}
    {/each}
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
  data-testid="delete-memory-confirm"
/>
