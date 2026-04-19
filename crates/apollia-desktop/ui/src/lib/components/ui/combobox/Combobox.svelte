<script lang="ts">
  import { Combobox as ComboboxPrimitive } from "bits-ui";
  import { cn } from "$lib/utils";
  import { t } from "svelte-i18n";
  import { Check, ChevronDown, Search as SearchIcon } from "lucide-svelte";
  import type { Component } from "svelte";

  export interface ComboboxItem {
    value: string;
    label: string;
    icon?: Component;
    disabled?: boolean;
  }

  interface Props {
    items: ComboboxItem[];
    value?: string;
    placeholder?: string;
    /** Override empty-state label (defaults to i18n `combobox.empty`). */
    emptyLabel?: string;
    /** Override search input placeholder (defaults to i18n `combobox.search_placeholder`). */
    searchPlaceholder?: string;
    /** Toggle the inline search input. */
    search?: boolean;
    disabled?: boolean;
    class?: string;
    name?: string;
  }

  let {
    items,
    value = $bindable(""),
    placeholder,
    emptyLabel,
    searchPlaceholder,
    search = true,
    disabled = false,
    class: className = "",
    name,
  }: Props = $props();

  let searchValue = $state("");

  const filteredItems = $derived(
    search && searchValue.trim().length > 0
      ? items.filter((item) =>
          item.label.toLowerCase().includes(searchValue.trim().toLowerCase()),
        )
      : items,
  );

  const selectedLabel = $derived(
    items.find((item) => item.value === value)?.label ?? "",
  );

  const resolvedPlaceholder = $derived(placeholder ?? $t("combobox.placeholder"));
  const resolvedEmpty = $derived(emptyLabel ?? $t("combobox.empty"));
  const resolvedSearch = $derived(
    searchPlaceholder ?? $t("combobox.search_placeholder"),
  );
</script>

<ComboboxPrimitive.Root
  type="single"
  bind:value
  {disabled}
  {name}
  onOpenChange={(open) => {
    if (!open) searchValue = "";
  }}
>
  <div class={cn("relative", className)}>
    {#if search}
      <span
        class="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground"
        aria-hidden="true"
      >
        <SearchIcon size={14} />
      </span>
      <ComboboxPrimitive.Input
        aria-label={resolvedSearch}
        placeholder={selectedLabel || resolvedPlaceholder}
        class={cn(
          "flex h-10 w-full rounded-md border border-border bg-background px-3 py-2 text-sm ring-offset-background transition-shadow duration-150 placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40 focus-visible:border-primary/50 disabled:cursor-not-allowed disabled:opacity-50",
          "pl-9 pr-8",
        )}
        oninput={(e) => {
          searchValue = (e.currentTarget as HTMLInputElement).value;
        }}
      />
    {:else}
      <ComboboxPrimitive.Trigger
        class={cn(
          "flex h-10 w-full items-center justify-between rounded-md border border-border bg-background px-3 py-2 text-sm ring-offset-background transition-shadow duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40 focus-visible:border-primary/50 disabled:cursor-not-allowed disabled:opacity-50",
          "pr-8",
        )}
      >
        <span class={cn(!selectedLabel && "text-muted-foreground")}>
          {selectedLabel || resolvedPlaceholder}
        </span>
      </ComboboxPrimitive.Trigger>
    {/if}
    <span
      class="pointer-events-none absolute right-2.5 top-1/2 -translate-y-1/2 text-muted-foreground"
      aria-hidden="true"
    >
      <ChevronDown size={14} strokeWidth={1.75} />
    </span>
  </div>

  <ComboboxPrimitive.Portal>
    <ComboboxPrimitive.Content
      sideOffset={4}
      style="z-index: var(--z-overlay);"
      class="glass-card glass-border rounded-lg shadow-lg p-1 min-w-[var(--bits-combobox-anchor-width)] max-h-[min(var(--bits-combobox-anchor-height,320px),320px)] overflow-y-auto"
    >
      {#if filteredItems.length === 0}
        <div
          class="px-3 py-6 text-center text-sm text-muted-foreground"
          role="status"
        >
          {resolvedEmpty}
        </div>
      {:else}
        {#each filteredItems as item (item.value)}
          <ComboboxPrimitive.Item
            value={item.value}
            label={item.label}
            disabled={item.disabled}
            class="flex w-full cursor-default items-center gap-2 rounded-md px-2.5 py-1.5 text-sm text-card-foreground outline-none data-[highlighted]:bg-muted data-[selected]:text-primary data-[disabled]:pointer-events-none data-[disabled]:opacity-50"
          >
            {#snippet children({ selected })}
              {#if item.icon}
                {@const IconComponent = item.icon}
                <IconComponent size={14} aria-hidden="true" />
              {/if}
              <span class="flex-1 truncate">{item.label}</span>
              {#if selected}
                <Check size={14} aria-hidden="true" />
              {/if}
            {/snippet}
          </ComboboxPrimitive.Item>
        {/each}
      {/if}
    </ComboboxPrimitive.Content>
  </ComboboxPrimitive.Portal>
</ComboboxPrimitive.Root>
