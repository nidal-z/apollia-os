<script lang="ts">
  /**
   * AddEntryCard - the card of the entry chosen in the add window, in its
   * centre: its name and who publishes it, what it is, what adding it asks,
   * then Add (or Manage when it is already here) and Cancel, which gives the
   * search back as it was typed. The address of a service holds its form in
   * the card.
   */
  import { locale, t } from "svelte-i18n";
  import { ArrowLeft, Check } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { NATIVE_PROVIDER_KEYS } from "$lib/connections/connectionRows";
  import { entryDescription, entryLabel } from "$lib/connections/catalogueText";
  import { vendorLabel } from "$lib/connections/vendor";
  import { entryNeeds, type AddEntry, type AddNeed } from "$lib/connections/addWindow";
  import CustomMcpForm from "./CustomMcpForm.svelte";

  interface Props {
    entry: AddEntry;
    /** The catalogue entry is already a connection of this machine. */
    installed: boolean;
    official: boolean;
    onadd: () => void;
    onmanage: () => void;
    /** Back to the search, the words kept. */
    oncancel: () => void;
    /** The form of the address added its connection. */
    oninstalled: () => void;
  }

  let { entry, installed, official, onadd, onmanage, oncancel, oninstalled }: Props = $props();

  const name = $derived.by((): string => {
    switch (entry.kind) {
      case "native":
        return $t(NATIVE_PROVIDER_KEYS[entry.provider].name);
      case "folder":
        return $t("connections.folder.name");
      case "mailbox":
        return $t("connections.mailbox.entry_name");
      case "address":
        return $t("connections.add_window.address_name");
      case "catalogue":
        return entryLabel(entry.entry, $locale);
    }
  });

  const description = $derived.by((): string => {
    switch (entry.kind) {
      case "native":
        return $t(NATIVE_PROVIDER_KEYS[entry.provider].description);
      case "folder":
        return $t("connections.folder.description");
      case "mailbox":
        return $t("connections.mailbox.entry_description");
      case "address":
        return $t("connections.add_window.address_description");
      case "catalogue":
        return entryDescription(entry.entry, $locale) ?? entry.entry.enrichment?.category ?? "";
    }
  });

  const NEED_KEYS: Record<AddNeed, string> = {
    sign_in: "connections.add_window.needs.sign_in",
    folder: "connections.add_window.needs.folder",
    mailbox: "connections.add_window.needs.mailbox",
    key: "connections.add_window.needs.key",
    values: "connections.add_window.needs.values",
    download: "connections.add_window.needs.download",
    online: "connections.add_window.needs.online",
    address: "connections.add_window.needs.address",
    nothing: "connections.add_window.needs.nothing",
  };

  const vendor = $derived(entry.kind === "catalogue" ? vendorLabel(entry.entry) : "");
  const needs = $derived(entryNeeds(entry));
</script>

<div class="flex min-h-0 min-w-0 flex-1 flex-col gap-4 overflow-auto px-6 py-5" data-testid="catalogue-card">
  <header class="flex flex-col gap-2">
    <button
      type="button"
      class="-ml-1.5 flex w-fit items-center gap-1 rounded-md px-1.5 py-1 text-xs text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      onclick={oncancel}
    >
      <ArrowLeft size={13} aria-hidden="true" />
      {$t("connections.add_window.back")}
    </button>
    <div class="flex flex-wrap items-center gap-2">
      <h2 class="min-w-0 truncate text-lg font-semibold text-foreground">{name}</h2>
      {#if entry.kind === "folder"}
        <Badge size="sm" variant="primary" outline>{$t("connections.badge_builtin")}</Badge>
      {:else if entry.kind === "catalogue"}
        {#if official}
          <Badge size="sm" variant="primary">{$t("connections.badge_official")}</Badge>
        {:else}
          <Badge size="sm" variant="neutral" outline>{$t("connections.badge_community")}</Badge>
        {/if}
        {#if entry.source === "public"}
          <Badge size="sm" variant="warning" outline>{$t("connections.add_window.public_unverified")}</Badge>
        {/if}
        {#if installed}
          <Badge size="sm" variant="success">{$t("connections.badge_installed")}</Badge>
        {/if}
      {/if}
    </div>
    {#if vendor}
      <p class="text-caption text-muted-foreground">
        {$t("connections.catalogue_by")}
        <span class="font-medium text-foreground/80">{vendor}</span>
      </p>
    {/if}
  </header>

  {#if description}
    <p class="text-sm text-foreground">{description}</p>
  {/if}
  {#if entry.kind === "catalogue"}
    <p class="text-caption text-muted-foreground">{$t("connections.badge_rule")}</p>
    {#if entry.source === "public"}
      <p class="text-caption text-muted-foreground">{$t("connections.add_window.public_note")}</p>
    {/if}
  {/if}

  <section class="flex flex-col gap-2 rounded-lg border border-border p-4" data-testid="catalogue-card-needs">
    <h3 class="text-overline text-muted-foreground">{$t("connections.add_window.needs_title")}</h3>
    <ul class="flex flex-col gap-1.5">
      {#each needs as need (need)}
        <li class="flex items-start gap-2 text-sm text-foreground" data-need={need}>
          <Check size={14} class="mt-0.5 shrink-0 text-primary" aria-hidden="true" />
          <span>{$t(NEED_KEYS[need])}</span>
        </li>
      {/each}
    </ul>
  </section>

  {#if entry.kind === "address"}
    <div class="-mx-6">
      <CustomMcpForm {oninstalled} />
    </div>
  {/if}

  <footer class="mt-auto flex justify-end gap-2 pt-2">
    <Button variant="ghost" size="sm" onclick={oncancel} data-testid="catalogue-card-cancel">
      {$t("common.cancel")}
    </Button>
    {#if entry.kind === "catalogue" && installed}
      <Button variant="primary-solid" size="sm" onclick={onmanage} data-testid="catalogue-card-add">
        {$t("connections.add_window.open")}
      </Button>
    {:else if entry.kind !== "address"}
      <Button variant="primary-solid" size="sm" onclick={onadd} data-testid="catalogue-card-add">
        {$t("connections.add_window.add")}
      </Button>
    {/if}
  </footer>
</div>
