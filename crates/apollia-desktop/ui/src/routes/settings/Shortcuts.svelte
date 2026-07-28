<script lang="ts" context="module">
  export const meta = {
    title: "settings.nav.shortcuts",
    icon: "keyboard",
    group: "settings.nav.cluster_system",
    cluster: "system",
  } as const;
</script>

<script lang="ts">
  import { get } from "svelte/store";
  import { t } from "svelte-i18n";
  import { Search } from "lucide-svelte";
  import { Input } from "$lib/components/ui/input";
  import {
    SHORTCUTS,
    SHORTCUT_CATEGORIES,
    filterShortcuts,
    isMacPlatform,
    type ShortcutCategory,
    type ShortcutEntry,
    type ShortcutScope,
  } from "$lib/navigation/shortcuts";
  import SettingsSubPage from "../../components/settings/SettingsSubPage.svelte";
  import SettingsSection from "../../components/settings/SettingsSection.svelte";
  import ScopeBadge from "../../components/settings/shortcuts/ScopeBadge.svelte";
  import ShortcutCatalogRow from "../../components/settings/shortcuts/ShortcutCatalogRow.svelte";
  import { isReserved, overlapsGlobal } from "../../components/settings/shortcuts/coverage";

  const mac = isMacPlatform();

  const CATEGORY_LABEL_KEYS: Record<ShortcutCategory, string> = {
    global: "settings.shortcuts.category.global",
    navigation: "settings.shortcuts.category.navigation",
    chat: "settings.shortcuts.category.chat",
    settings: "settings.shortcuts.category.settings",
    companion: "settings.shortcuts.category.companion",
    approvals: "settings.shortcuts.category.approvals",
  };

  let query = $state("");

  let filtered = $derived.by<ShortcutEntry[]>(() => {
    const translate = (key: string) => get(t)(key);
    return filterShortcuts(SHORTCUTS, query, translate, mac);
  });

  let grouped = $derived.by(() => {
    const out = {} as Record<ShortcutCategory, ShortcutEntry[]>;
    for (const cat of SHORTCUT_CATEGORIES) out[cat] = [];
    for (const entry of filtered) out[entry.category].push(entry);
    return out;
  });

  let isEmpty = $derived(filtered.length === 0);

  /** A group's shared scope, or null when the group mixes global + contextual. */
  function uniformScope(entries: ShortcutEntry[]): ShortcutScope | null {
    if (entries.length === 0) return null;
    const first = entries[0].scope;
    return entries.every((e) => e.scope === first) ? first : null;
  }

  /** Per-row scope label used only when the group is mixed (no group badge). */
  function inlineScopeLabel(
    entry: ShortcutEntry,
    groupScope: ShortcutScope | null,
  ): string | null {
    if (groupScope !== null) return null;
    return entry.scope === "global"
      ? $t("settings.shortcuts.scope.global_label")
      : $t("settings.shortcuts.scope.contextual_label");
  }
</script>

<SettingsSubPage route="shortcuts" data-testid="shortcuts-section">
  <div class="flex flex-wrap items-center gap-3">
    <div class="relative min-w-[14rem] flex-1">
      <Search
        size={14}
        strokeWidth={1.75}
        class="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground"
        aria-hidden="true"
      />
      <Input
        type="search"
        bind:value={query}
        placeholder={$t("settings.shortcuts.search_placeholder")}
        aria-label={$t("settings.shortcuts.search_aria")}
        class="w-full pl-9"
        data-testid="shortcuts-search"
      />
    </div>
    <span class="shrink-0 text-caption text-muted-foreground">
      {$t("settings.shortcuts.count", { values: { count: filtered.length } })}
    </span>
  </div>

  {#if isEmpty}
    <div
      class="rounded-xl border border-border bg-muted/10 px-4 py-10 text-center text-body-sm text-muted-foreground"
      data-testid="shortcuts-empty"
    >
      {$t("settings.shortcuts.no_results", { values: { query } })}
    </div>
  {:else}
    {#each SHORTCUT_CATEGORIES as cat (cat)}
      {@const entries = grouped[cat]}
      {#if entries.length > 0}
        {@const groupScope = uniformScope(entries)}
        <SettingsSection
          title={$t(CATEGORY_LABEL_KEYS[cat])}
          bodyLayout="flush"
          data-testid="shortcuts-category-{cat}"
        >
          {#snippet actions()}
            {#if groupScope !== null}
              <ScopeBadge scope={groupScope} />
            {/if}
          {/snippet}
          {#each entries as entry (entry.id)}
            <ShortcutCatalogRow
              {entry}
              {mac}
              reserved={isReserved(entry)}
              overlaps={overlapsGlobal(entry)}
              scopeInline={inlineScopeLabel(entry, groupScope)}
            />
          {/each}
        </SettingsSection>
      {/if}
    {/each}
  {/if}
</SettingsSubPage>
