<script lang="ts" context="module">
  import { Card } from "$lib/components/ui/card";
  import { Input } from "$lib/components/ui/input";
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
  import { Keyboard, Search } from "lucide-svelte";
  import {
    SHORTCUTS,
    SHORTCUT_CATEGORIES,
    filterShortcuts,
    isMacPlatform,
    type ShortcutCategory,
    type ShortcutEntry,
  } from "$lib/navigation/shortcuts";
  import ShortcutRow from "../../components/settings/ShortcutRow.svelte";

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
    const translator = (key: string) => get(t)(key);
    return filterShortcuts(SHORTCUTS, query, translator, mac);
  });

  let grouped = $derived.by(() => {
    const out: Record<ShortcutCategory, ShortcutEntry[]> = {
      global: [],
      navigation: [],
      chat: [],
      settings: [],
      companion: [],
      approvals: [],
    };
    for (const entry of filtered) out[entry.category].push(entry);
    return out;
  });

  let isEmpty = $derived(filtered.length === 0);
</script>

<section class="space-y-4" data-testid="shortcuts-section">
  <div class="flex items-start gap-3">
    <Keyboard size={20} strokeWidth={1.75} class="mt-0.5 text-muted-foreground" aria-hidden="true" />
    <p class="text-sm text-muted-foreground">{$t("settings.shortcuts.subtitle")}</p>
  </div>

  <div class="relative">
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
      class="w-full rounded-md border border-border bg-muted/20 py-2 pl-9 pr-3 text-sm text-foreground placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      data-testid="shortcuts-search"
     />
  </div>

  {#if isEmpty}
    <div
      class="rounded-lg border border-border bg-muted/10 px-4 py-8 text-center text-sm text-muted-foreground"
      data-testid="shortcuts-empty"
    >
      {$t("settings.shortcuts.no_results", { values: { query } })}
    </div>
  {:else}
    {#each SHORTCUT_CATEGORIES as cat (cat)}
      {@const entries = grouped[cat]}
      {#if entries.length > 0}
        <div data-testid="shortcuts-category-{cat}">
          <h2
            class="mb-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground"
          >
            {$t(CATEGORY_LABEL_KEYS[cat])}
          </h2>
          <Card class="rounded-lg divide-y divide-border/50">
            {#each entries as entry (entry.id)}
              <ShortcutRow {entry} {mac} />
            {/each}
          </Card>
        </div>
      {/if}
    {/each}
  {/if}
</section>
