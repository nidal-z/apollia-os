<script lang="ts">
  /**
   * AddConnectionWindow - the window that adds a connection: one search field
   * across its width, then four groups (what Apollia offers, what is on this
   * computer, the public catalogue, a service by its address), and beside
   * them the card of the chosen entry, then, once Add is pressed, the one flow
   * of its addition in the card's place. The public catalogue is read only when
   * it is asked for, and is marked as not verified. A source that did not
   * answer says so in a banner under the search; it never reads as an empty
   * catalogue.
   */
  import { locale, t } from "svelte-i18n";
  import { Folder, Globe, Link as LinkIcon, Mail, Search, UserRound, X } from "lucide-svelte";
  import { Dialog } from "$lib/components/ui/dialog";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Spinner } from "$lib/components/ui/progress";
  import { ListRow } from "$lib/components/operator";
  import { developerMode } from "$lib/stores/developerMode";
  import BrowseLayout from "$lib/components/operator/BrowseLayout.svelte";
  import ConnectorTile from "../shared/ConnectorTile.svelte";
  import AddEntryCard from "./AddEntryCard.svelte";
  import AddFlow from "../add/AddFlow.svelte";
  import { rankSuggestions } from "../ConnectionSuggestions.svelte";
  import { NATIVE_PROVIDERS, NATIVE_PROVIDER_KEYS } from "$lib/connections/connectionRows";
  import { accentTokenFor } from "$lib/connections/accent";
  import { entryDescription, entryLabel } from "$lib/connections/catalogueText";
  import { installedNameFor, isInstalled, isOfficial } from "$lib/connections/vendor";
  import type { CatalogueFailure, CatalogueTier } from "$lib/connections/catalogueSource";
  import {
    addWindowGroups,
    findEntry,
    nativeKey,
    type AddEntry,
    type AddGroupKey,
  } from "$lib/connections/addWindow";
  import type { ProviderId } from "$lib/ipc/connections";
  import type { AgentListItem, RegistryServerView } from "$lib/types";

  interface Props {
    open: boolean;
    query: string;
    /** The key of the entry whose card is open; null shows the groups alone. */
    chosen: string | null;
    /** Add was pressed on the card: its flow stands in the card's place. */
    adding: boolean;
    /** The connection a sign-in done in its own dialog added: the flow resumes on it. */
    resume: string | null;
    /** The address of a mailbox whose password is typed again, empty for a new one. */
    mailbox?: string;
    selection: RegistryServerView[];
    publicEntries: RegistryServerView[];
    failures: CatalogueFailure[];
    catalogueTier: CatalogueTier;
    loading: boolean;
    agents: AgentListItem[];
    installedNames: Set<string>;
    installedByPackage: Map<string, string>;
    onclose: () => void;
    onchoose: (key: string) => void;
    /** Cancel on the card: back to the search. */
    onback: () => void;
    /** Read the public catalogue: the only gesture that asks it. */
    onexploreall: () => void;
    /** Show the selection alone again. */
    onbacktoselection: () => void;
    /** Add on the card of `entry`: its flow starts. */
    onadd: (entry: AddEntry) => void;
    /** Open the page of a connection already here. */
    onmanage: (name: string) => void;
    /** The flow added its connection. */
    onadded: (connectionId: string) => void;
    /** The flow is over, on the connection it added when it added one. */
    ondone: (connectionId: string | null) => void;
    /** Sign in to a native account, in the dialog of its provider. */
    onsignin: (provider: ProviderId) => void;
    ontry: (connectionId: string, name: string) => void;
    ontryprompt: (connectionId: string, prompt: string) => void;
    onopenrule: () => void;
  }

  let {
    open,
    query = $bindable(""),
    chosen,
    adding,
    resume,
    mailbox = "",
    selection,
    publicEntries,
    failures,
    catalogueTier,
    loading,
    agents,
    installedNames,
    installedByPackage,
    onclose,
    onchoose,
    onback,
    onexploreall,
    onbacktoselection,
    onadd,
    onmanage,
    onadded,
    ondone,
    onsignin,
    ontry,
    ontryprompt,
    onopenrule,
  }: Props = $props();

  const PAGE = 48;
  let limit = $state(PAGE);
  $effect(() => {
    void query;
    limit = PAGE;
  });

  const GROUP_KEYS: Record<AddGroupKey, string> = {
    apollia: "connections.add_window.groups.apollia",
    computer: "connections.add_window.groups.computer",
    public: "connections.add_window.groups.public",
    address: "connections.add_window.groups.address",
  };

  /** The selection, the entries that suit the agents of this machine first. */
  const ranked = $derived.by(() => {
    const first = rankSuggestions(selection, agents, 12);
    const seen = new Set(first.map((e) => e.name));
    return [...first, ...selection.filter((e) => !seen.has(e.name))];
  });

  function wordsOf(key: string): string[] {
    if (key === "folder") return [$t("connections.folder.name"), $t("connections.folder.description")];
    if (key === "mailbox") {
      return [$t("connections.mailbox.entry_name"), $t("connections.mailbox.entry_description")];
    }
    for (const provider of NATIVE_PROVIDERS) {
      if (key === nativeKey(provider)) {
        return [$t(NATIVE_PROVIDER_KEYS[provider].name), $t(NATIVE_PROVIDER_KEYS[provider].description)];
      }
    }
    return [];
  }

  const groups = $derived(
    addWindowGroups({ selection: ranked, publicEntries, query, wordsOf }),
  );
  const chosenEntry = $derived(findEntry(groups, chosen));
  const searched = $derived(query.trim().length > 0);
  const nothingFound = $derived(
    searched && groups.every((g) => g.key === "address" || g.entries.length === 0),
  );

  const failureMessage = $derived.by((): string | null => {
    const words: string[] = [];
    if (failures.includes("selection")) words.push($t("connections.add_window.failed_selection"));
    if (failures.includes("public")) words.push($t("connections.add_window.failed_public"));
    return words.length > 0 ? words.join(" ") : null;
  });

  function installedOf(entry: AddEntry): boolean {
    return entry.kind === "catalogue" && isInstalled(entry.entry, installedNames, installedByPackage);
  }

  function manage(entry: AddEntry): void {
    if (entry.kind !== "catalogue") return;
    onmanage(installedNameFor(entry.entry, installedNames, installedByPackage) ?? entry.entry.name);
  }

  function rowName(entry: AddEntry): string {
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
  }

  function rowDescription(entry: AddEntry): string {
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
  }
</script>

<!-- The anchor of each row sits on its item, spelt out per kind. -->
{#snippet row(entry: AddEntry)}
  {#if entry.kind === "catalogue"}
    <li data-testid="catalogue-entry-{entry.entry.name}">{@render rowBody(entry)}</li>
  {:else if entry.kind === "native"}
    <li data-testid="catalogue-native-{entry.provider}">{@render rowBody(entry)}</li>
  {:else if entry.kind === "folder"}
    <li data-testid="catalogue-folder">{@render rowBody(entry)}</li>
  {:else if entry.kind === "mailbox"}
    <li data-testid="catalogue-native-mailbox">{@render rowBody(entry)}</li>
  {:else}
    <li data-testid="catalogue-address">{@render rowBody(entry)}</li>
  {/if}
{/snippet}

{#snippet rowBody(entry: AddEntry)}
    <ListRow
      variant="row"
      border={false}
      pad="snug"
      class="rounded-lg border border-border/60 bg-card elev-interactive hover:border-primary/40"
      state={entry.key === chosen ? "active" : "default"}
      onclick={() => onchoose(entry.key)}
    >
      <ConnectorTile
        size="md"
        accent={entry.kind === "catalogue" ? accentTokenFor(rowName(entry)) : "hsl(var(--primary))"}
      >
        {#snippet icon()}
          {#if entry.kind === "folder"}<Folder size={14} />
          {:else if entry.kind === "mailbox"}<Mail size={14} />
          {:else if entry.kind === "native"}<UserRound size={14} />
          {:else if entry.kind === "address"}<Globe size={14} />
          {:else}<LinkIcon size={14} />{/if}
        {/snippet}
      </ConnectorTile>
      <div class="min-w-0 flex-1">
        <div class="flex min-w-0 items-center gap-1.5">
          <span class="truncate text-body-sm font-medium text-foreground">{rowName(entry)}</span>
          {#if entry.kind === "catalogue"}
            {#if isOfficial(entry.entry)}
              <Badge size="sm" variant="primary" data-testid="catalogue-badge-official"
                >{$t("connections.badge_official")}</Badge
              >
            {:else}
              <Badge size="sm" variant="neutral" outline data-testid="catalogue-badge-community"
                >{$t("connections.badge_community")}</Badge
              >
            {/if}
            {#if installedOf(entry)}
              <Badge size="sm" variant="success">{$t("connections.badge_installed")}</Badge>
            {/if}
          {/if}
        </div>
        <div class="truncate text-caption text-muted-foreground">{rowDescription(entry)}</div>
      </div>
    </ListRow>
{/snippet}

<Dialog
  {open}
  {onclose}
  size="xl"
  label={$t("connections.add_connection")}
  bodyClass="p-0"
  hideClose
  data-testid="catalogue-window"
>
  <div class="flex h-[min(780px,88vh)] min-h-0">
    <BrowseLayout
      detailOpen={chosenEntry !== null}
      offlineMessage={failureMessage}
      listTestid="catalogue-list"
      offlineTestid="catalogue-failure"
    >
      {#snippet header()}
        <div class="flex items-start gap-3">
          <div class="min-w-0 flex-1">
            <h2 class="text-lg font-semibold text-foreground">{$t("connections.add_connection")}</h2>
            <p class="mt-0.5 text-sm text-muted-foreground">
              {$developerMode ? $t("dev.connections.catalogue_subtitle") : $t("connections.add_window.subtitle")}
            </p>
          </div>
          <button
            type="button"
            class="flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
            onclick={onclose}
            aria-label={$t("common.close")}
            title={$t("common.close")}
            data-testid="catalogue-close"
          >
            <X size={16} aria-hidden="true" />
          </button>
        </div>
        <Input
          icon={Search}
          size="sm"
          type="search"
          bind:value={query}
          placeholder={$t("connections.catalogue_search_placeholder")}
          aria-label={$t("connections.catalogue_search_placeholder")}
          data-testid="catalogue-search"
        />
      {/snippet}

      <div class="flex flex-col gap-5">
        {#each groups as group (group.key)}
          {#if group.key === "apollia"}
            <section class="flex flex-col gap-2" data-testid="catalogue-group-{group.key}">
              <h3 class="text-overline text-muted-foreground">{$t(GROUP_KEYS[group.key])}</h3>
              {#if loading && selection.length === 0 && failures.length === 0}
                <div class="flex items-center gap-2 text-body-xs text-muted-foreground" data-testid="catalogue-loading">
                  <Spinner size={12} />
                  {$developerMode ? $t("dev.connections.catalogue_loading") : $t("connections.catalogue_loading")}
                </div>
              {/if}
              {#if group.entries.length > 0}
                <ul class="space-y-1.5">
                  {#each group.entries as entry (entry.key)}{@render row(entry)}{/each}
                </ul>
              {/if}
            </section>
          {:else if group.key === "computer"}
            {#if group.entries.length > 0}
              <section class="flex flex-col gap-2" data-testid="catalogue-group-{group.key}">
                <h3 class="text-overline text-muted-foreground">{$t(GROUP_KEYS[group.key])}</h3>
                <ul class="space-y-1.5">
                  {#each group.entries as entry (entry.key)}{@render row(entry)}{/each}
                </ul>
              </section>
            {/if}
          {:else if group.key === "public"}
            <section class="flex flex-col gap-2" data-testid="catalogue-group-{group.key}">
              <div class="flex items-center gap-2">
                <h3 class="text-overline text-muted-foreground">{$t(GROUP_KEYS[group.key])}</h3>
                <Badge size="sm" variant="warning" outline>{$t("connections.add_window.public_unverified")}</Badge>
              </div>
              {#if catalogueTier === "full"}
                <p class="text-caption text-muted-foreground">{$t("connections.add_window.public_note")}</p>
                {#if group.entries.length > 0}
                  <ul class="space-y-1.5">
                    {#each group.entries.slice(0, limit) as entry (entry.key)}{@render row(entry)}{/each}
                  </ul>
                  {#if group.entries.length > limit}
                    <Button
                      variant="ghost"
                      size="sm"
                      class="self-center"
                      onclick={() => (limit += PAGE)}
                      data-testid="catalogue-more-hint"
                    >
                      {$t("connections.add_window.show_more", {
                        values: { count: Math.min(PAGE, group.entries.length - limit) },
                      })}
                    </Button>
                  {/if}
                {/if}
                <Button
                  variant="ghost"
                  size="sm"
                  class="self-end"
                  onclick={onbacktoselection}
                  data-testid="catalogue-back-to-selection"
                >
                  {$t("connections.catalogue_back_to_selection")}
                </Button>
              {:else}
                <div class="flex flex-wrap items-center justify-between gap-3">
                  <p class="text-caption text-muted-foreground">{$t("connections.add_window.public_ask")}</p>
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={loading}
                    onclick={onexploreall}
                    data-testid="catalogue-explore-all"
                  >
                    {#if loading}<Spinner size={11} />{/if}
                    {$t("connections.catalogue_explore_all")}
                  </Button>
                </div>
              {/if}
            </section>
          {:else}
            <section class="flex flex-col gap-2" data-testid="catalogue-group-{group.key}">
              <h3 class="text-overline text-muted-foreground">
                {$developerMode ? $t("dev.connections.catalogue_tab_custom") : $t(GROUP_KEYS[group.key])}
              </h3>
              {#if nothingFound}
                <p class="text-caption text-muted-foreground">
                  {$t("connections.add_window.no_results", { values: { query: query.trim() } })}
                </p>
              {/if}
              <ul class="space-y-1.5">
                {#each group.entries as entry (entry.key)}{@render row(entry)}{/each}
              </ul>
            </section>
          {/if}
        {/each}
      </div>

      {#snippet detail()}
        {#if chosenEntry && (adding || chosenEntry.kind === "address")}
          <!-- The address has nothing to read before its form: its flow opens
               with it. Keyed on the entry, so another entry starts afresh. -->
          {#key chosenEntry.key}
            <AddFlow
              entry={chosenEntry}
              name={rowName(chosenEntry)}
              accepted={chosenEntry.kind === "catalogue"}
              {resume}
              mailboxAddress={mailbox}
              {onback}
              {onadded}
              {ondone}
              {onsignin}
              {ontry}
              {ontryprompt}
              {onopenrule}
            />
          {/key}
        {:else if chosenEntry}
          <AddEntryCard
            entry={chosenEntry}
            installed={installedOf(chosenEntry)}
            official={chosenEntry.kind === "catalogue" && isOfficial(chosenEntry.entry)}
            onadd={() => onadd(chosenEntry)}
            onmanage={() => manage(chosenEntry)}
            oncancel={onback}
          />
        {/if}
      {/snippet}
    </BrowseLayout>
  </div>
</Dialog>
