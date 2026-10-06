<script lang="ts">
  /**
   * The tree of the documentation, in the column of the developer screen:
   * every page of the archive, in the order and under the categories of the
   * public site.
   */
  import { locale, t } from "svelte-i18n";
  import { ChevronDown, ChevronRight } from "lucide-svelte";
  import { cn } from "$lib/utils";
  import type { DocumentationTreeItem } from "$lib/ipc/documentation";
  import { developerMode } from "$lib/stores/developerMode";
  import {
    categoriesHolding,
    categoryKey,
    categoryOpen,
    categorySlug,
    entrySlug,
  } from "./documentation";
  import { documentationReader } from "./documentationReader";

  interface Props {
    /** Whether the documentation is the view shown: the page read is marked only then. */
    active: boolean;
    /** Called when the user chose a page. */
    onopen: () => void;
  }

  let { active, onopen }: Props = $props();

  const tree = $derived($documentationReader.manifest?.tree ?? []);
  const route = $derived($documentationReader.route);
  const holding = $derived(categoriesHolding(tree, route));
  /** The categories the user opened or closed, by key. */
  let toggled = $state<Record<string, boolean>>({});

  // The documentation follows the language of the interface.
  $effect(() => {
    if ($developerMode) void documentationReader.start($locale ?? "");
  });

  function openPage(target: string): void {
    void documentationReader.open(target);
    onopen();
  }

  function chooseCategory(key: string, target: string | null): void {
    const shown = active && target !== null && route === target;
    if (target !== null && !shown) {
      toggled = { ...toggled, [key]: true };
      openPage(target);
      return;
    }
    toggled = { ...toggled, [key]: !categoryOpen(key, holding, toggled) };
  }

  const ROW =
    "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60";
</script>

{#snippet entries(items: readonly DocumentationTreeItem[], parent: string)}
  <ul class={cn("flex flex-col gap-0.5", parent !== "" && "pl-3")}>
    {#each items as item, index (index)}
      {@const current = active && item.route !== null && item.route === route}
      <li>
        {#if item.kind === "category"}
          {@const key = categoryKey(parent, index)}
          {@const open = categoryOpen(key, holding, toggled)}
          <button
            type="button"
            class={cn(
              ROW,
              "justify-between font-medium",
              current ? "bg-muted text-foreground" : "text-foreground hover:bg-muted",
            )}
            aria-expanded={open}
            aria-current={current ? "page" : undefined}
            onclick={() => chooseCategory(key, item.route)}
            data-testid={`developer-docs-category-${categorySlug(key)}`}
          >
            <span class="min-w-0">{item.label}</span>
            {#if open}
              <ChevronDown size={14} class="shrink-0 text-muted-foreground" aria-hidden="true" />
            {:else}
              <ChevronRight size={14} class="shrink-0 text-muted-foreground" aria-hidden="true" />
            {/if}
          </button>
          {#if open}
            {@render entries(item.items, key)}
          {/if}
        {:else}
          <button
            type="button"
            class={cn(
              ROW,
              current
                ? "bg-muted font-medium text-foreground"
                : "text-muted-foreground hover:bg-muted hover:text-foreground",
            )}
            aria-current={current ? "page" : undefined}
            onclick={() => openPage(item.route)}
            data-testid={`developer-docs-entry-${entrySlug(item.route)}`}
          >
            {item.label}
          </button>
        {/if}
      </li>
    {/each}
  </ul>
{/snippet}

{#if $developerMode}
  <nav
    class="min-h-0 flex-1 overflow-y-auto px-2 py-2"
    aria-label={$t("dev.developer.docs.tree_label")}
    data-testid="developer-docs-tree"
  >
    {@render entries(tree, "")}
  </nav>
{/if}
