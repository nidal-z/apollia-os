<script lang="ts">
  /**
   * AddConnectionWindow - the window that adds a connection, browsed by
   * usage: across its width the usages, the search, the source, the order and
   * "no setup"; under them the services gathered by usage, each group folded
   * or not, then a service by its address; beside them the card of the chosen
   * entry, then, once Add is pressed, the one flow of its addition in the
   * card's place. The public catalogue is read only when the source includes
   * it, or searched by the text typed, and each of its entries is marked as
   * community. A source that did not answer says so in a banner under the
   * search; it never reads as an empty catalogue.
   */
  import { locale, t } from "svelte-i18n";
  import { ChevronDown, ChevronRight, Cloud, Folder, Globe, KeyRound, Mail, Monitor, ShieldCheck, UserRound, X, Zap } from "lucide-svelte";
  import { cn } from "$lib/utils";
  import { Dialog } from "$lib/components/ui/dialog";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { ListRow } from "$lib/components/operator";
  import { developerMode } from "$lib/stores/developerMode";
  import BrowseLayout from "$lib/components/operator/BrowseLayout.svelte";
  import BrandLogo from "../shared/BrandLogo.svelte";
  import AddEntryCard from "./AddEntryCard.svelte";
  import AddBrowseBar from "./AddBrowseBar.svelte";
  import AddFlow from "../add/AddFlow.svelte";
  import { rankSuggestions } from "../ConnectionSuggestions.svelte";
  import { NATIVE_PROVIDERS, NATIVE_PROVIDER_KEYS } from "$lib/connections/connectionRows";
  import { brandLogoPath, nativeLogoPath } from "$lib/connections/brandLogo";
  import { usageKey, type Usage } from "$lib/connections/usages";
  import {
    browseCatalogue,
    entryAuth,
    entryPlace,
    isVerified,
    sourceShowsPublic,
    usageCounts,
    type EntryAuth,
  } from "$lib/connections/addBrowse";
  import { categoryKey } from "$lib/connections/categories";
  import { entryDescription, entryLabel } from "$lib/connections/catalogueText";
  import { installedNameFor, isInstalled, isOfficial } from "$lib/connections/vendor";
  import type { CatalogueFailure, CatalogueTier } from "$lib/connections/catalogueSource";
  import {
    BROWSE_DEFAULT,
    addWindowGroups,
    findEntry,
    nativeKey,
    type AddEntry,
    type BrowseSource,
    type BrowseState,
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
    /** How the list is browsed: usage, source, order, "no setup". */
    browse?: BrowseState;
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
    /** Read the public catalogue: asked when the source chosen includes it. */
    onexploreall: () => void;
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
    browse = $bindable({ ...BROWSE_DEFAULT }),
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
    onadd,
    onmanage,
    onadded,
    ondone,
    onsignin,
    ontry,
    ontryprompt,
    onopenrule,
  }: Props = $props();

  /** Rows shown per group before "Show more", so a public catalogue of thousands stays light. */
  const PAGE = 48;
  let limits = $state<Record<string, number>>({});
  $effect(() => {
    void query;
    void browse;
    limits = {};
  });
  const limitOf = (group: string): number => limits[group] ?? PAGE;
  function showMore(group: string): void {
    limits = { ...limits, [group]: limitOf(group) + PAGE };
  }

  // The usages the person folded, for the time the window stays mounted.
  let folded = $state<Set<Usage>>(new Set());
  function toggle(usage: Usage): void {
    const next = new Set(folded);
    if (!next.delete(usage)) next.add(usage);
    folded = next;
  }

  /** Another source: the public catalogue is read the first time one includes it. */
  function chooseSource(source: BrowseSource): void {
    browse = { ...browse, source };
    if (sourceShowsPublic(source) && catalogueTier !== "full") onexploreall();
  }

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

  function categoryWords(category: string): string[] {
    const key = categoryKey(category);
    return key ? [$t(key)] : [];
  }

  const groups = $derived(
    addWindowGroups({ selection: ranked, publicEntries, query, wordsOf, categoryWords }),
  );
  const chosenEntry = $derived(findEntry(groups, chosen));
  const searched = $derived(query.trim().length > 0);
  const view = $derived(browseCatalogue({ groups, state: browse, searched, nameOf: rowName, locale: $locale }));
  const counts = $derived(usageCounts({ groups, state: browse, searched, nameOf: rowName, locale: $locale }));
  /** Nothing answers: the empty state stands, and it offers the address. */
  const emptyShown = $derived(view.empty && !loading);
  const address = $derived(groups.find((g) => g.key === "address")?.entries ?? []);

  /** Cards side by side across the width; one column beside an open card. */
  const gridClass = $derived(
    chosenEntry !== null ? "grid grid-cols-1 gap-2.5" : "grid grid-cols-[repeat(auto-fill,minmax(16rem,1fr))] gap-3",
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

  const AUTH_SHORT: Record<EntryAuth, string> = {
    account: "connections.add_window.card.auth_short.account",
    account_or_key: "connections.add_window.card.auth_short.account_or_key",
    key: "connections.add_window.card.auth_short.key",
    values: "connections.add_window.card.auth_short.values",
    folder: "connections.add_window.card.auth_short.folder",
    mailbox: "connections.add_window.card.auth_short.mailbox",
    keyless: "connections.add_window.card.auth_short.keyless",
  };

  /** What every card shares, whatever its kind: the chosen state and the choice. */
  function rowProps(entry: AddEntry) {
    const active = entry.key === chosen;
    return {
      variant: "row" as const,
      border: false,
      align: "stretch" as const,
      class: cn(
        "h-full flex-col gap-3 rounded-xl border p-3.5 shadow-sm transition-[border-color,box-shadow,background-color]",
        active
          ? "border-primary/50 bg-primary/5 ring-1 ring-primary/30"
          : "border-border bg-card hover:border-primary/30 hover:bg-card hover:shadow-md",
      ),
      state: active ? ("active" as const) : ("default" as const),
      onclick: () => onchoose(entry.key),
    };
  }
</script>

<!-- The anchor of each row sits on the control that chooses it, spelt out per
     kind: a click dispatched on the anchor itself only bubbles up, so an anchor
     on the item around the control would choose nothing. -->
{#snippet row(entry: AddEntry)}
  {#if entry.kind === "catalogue"}
    <li><ListRow {...rowProps(entry)} data-testid="catalogue-entry-{entry.entry.name}">{@render rowBody(entry)}</ListRow></li>
  {:else if entry.kind === "native"}
    <li><ListRow {...rowProps(entry)} data-testid="catalogue-native-{entry.provider}">{@render rowBody(entry)}</ListRow></li>
  {:else if entry.kind === "folder"}
    <li><ListRow {...rowProps(entry)} data-testid="catalogue-folder">{@render rowBody(entry)}</ListRow></li>
  {:else if entry.kind === "mailbox"}
    <li><ListRow {...rowProps(entry)} data-testid="catalogue-native-mailbox">{@render rowBody(entry)}</ListRow></li>
  {:else}
    <li><ListRow {...rowProps(entry)} data-testid="catalogue-address">{@render rowBody(entry)}</ListRow></li>
  {/if}
{/snippet}

{#snippet rowBody(entry: AddEntry)}
  {@const logo =
    entry.kind === "catalogue"
      ? brandLogoPath(entry.entry.enrichment?.icon_name)
      : entry.kind === "native"
        ? nativeLogoPath(entry.provider)
        : null}
  {@const auth = entryAuth(entry)}
  {@const place = entryPlace(entry)}
  <div class="flex min-w-0 items-start gap-3">
    {#if entry.kind === "catalogue"}
      <BrandLogo path={logo} name={rowName(entry)} data-testid={logo ? `catalogue-logo-${entry.entry.name}` : undefined} />
    {:else if entry.kind === "native"}
      <BrandLogo path={logo} name={rowName(entry)} data-testid={logo ? `catalogue-logo-native-${entry.provider}` : undefined}>
        {#snippet icon()}<UserRound size={16} strokeWidth={1.75} aria-hidden="true" />{/snippet}
      </BrandLogo>
    {:else}
      <BrandLogo name={rowName(entry)}>
        {#snippet icon()}
          {#if entry.kind === "folder"}<Folder size={16} strokeWidth={1.75} aria-hidden="true" />
          {:else if entry.kind === "mailbox"}<Mail size={16} strokeWidth={1.75} aria-hidden="true" />
          {:else}<Globe size={16} strokeWidth={1.75} aria-hidden="true" />{/if}
        {/snippet}
      </BrandLogo>
    {/if}
    <div class="min-w-0 flex-1">
      <div class="flex min-w-0 items-center gap-1.5">
        <span class="truncate text-body-sm font-semibold text-foreground">{rowName(entry)}</span>
        {#if entry.kind === "catalogue" && isVerified(entry)}
          <span
            class="inline-flex shrink-0 text-primary"
            title={$t("connections.add_window.browse.badge_verified")}
            data-testid="catalogue-badge-verified"
          >
            <ShieldCheck size={14} aria-hidden="true" />
            <span class="sr-only">{$t("connections.add_window.browse.badge_verified")}</span>
          </span>
        {/if}
      </div>
      <p class="mt-0.5 line-clamp-2 text-caption leading-snug text-muted-foreground">{rowDescription(entry)}</p>
    </div>
  </div>
  <div class="mt-auto flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 border-t border-border/60 pt-2.5 text-caption text-muted-foreground">
    <span class="inline-flex items-center gap-1" title={$t("connections.add_window.card.auth_label")}>
      {#if auth === "account" || auth === "account_or_key"}<UserRound size={12} aria-hidden="true" />
      {:else if auth === "keyless"}<Zap size={12} aria-hidden="true" />
      {:else if auth === "folder"}<Folder size={12} aria-hidden="true" />
      {:else if auth === "mailbox"}<Mail size={12} aria-hidden="true" />
      {:else}<KeyRound size={12} aria-hidden="true" />{/if}
      {$t(AUTH_SHORT[auth])}
    </span>
    <span class="inline-flex items-center gap-1" title={$t("connections.add_window.card.place_label")} data-place={place}>
      {#if place === "remote"}<Cloud size={12} aria-hidden="true" />{:else}<Monitor size={12} aria-hidden="true" />{/if}
      {place === "remote" ? $t("connections.remote") : $t("connections.machine")}
    </span>
    <span class="ml-auto flex items-center gap-1">
      {#if entry.kind === "catalogue" && !isVerified(entry)}
        <Badge size="sm" variant="neutral" outline data-testid="catalogue-badge-community"
          >{$t("connections.add_window.browse.badge_community")}</Badge
        >
      {/if}
      {#if installedOf(entry)}
        <Badge size="sm" variant="success">{$t("connections.badge_installed")}</Badge>
      {/if}
    </span>
  </div>
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
      stack
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
        <AddBrowseBar bind:query bind:browse {counts} {loading} onsource={chooseSource} />
      {/snippet}

      <div class="flex flex-col gap-5">
        {#if browse.source !== "verified" && view.communityShown}
          <p class="text-caption text-muted-foreground" data-testid="catalogue-public-note">
            {$t("connections.add_window.browse.community_note")}
          </p>
        {/if}

        {#if emptyShown}
          <div class="flex flex-col items-start gap-2 rounded-lg border border-dashed border-border p-4" data-testid="catalogue-empty">
            <p class="text-sm text-foreground">
              {#if searched}
                {$t("connections.add_window.browse.empty_query", { values: { query: query.trim() } })}
              {:else if browse.usage !== "all"}
                {$t("connections.add_window.browse.empty_usage")}
              {:else}
                {$t("connections.add_window.browse.empty")}
              {/if}
            </p>
            <div class="flex flex-wrap gap-2">
              {#if browse.source === "verified"}
                <Button variant="outline" size="sm" onclick={() => chooseSource("all")} data-testid="catalogue-empty-public">
                  {$t("connections.add_window.browse.empty_public")}
                </Button>
              {/if}
              <Button variant="ghost" size="sm" onclick={() => onchoose("address")} data-testid="catalogue-empty-address">
                {$t("connections.add_window.browse.empty_address")}
              </Button>
            </div>
          </div>
        {/if}

        {#each view.groups as group (group.usage)}
          {@const unfolded = !folded.has(group.usage)}
          <section class="flex flex-col gap-2" data-testid="catalogue-usage-{group.usage}" data-verified={group.entries.filter(isVerified).length}>
            <button
              type="button"
              class="flex w-full items-center gap-2 rounded-md px-1 py-1 text-left hover:bg-muted/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
              aria-expanded={unfolded}
              aria-label={$t("connections.add_window.browse.group_toggle", { values: { usage: $t(usageKey(group.usage)) } })}
              onclick={() => toggle(group.usage)}
              data-testid="catalogue-usage-toggle-{group.usage}"
            >
              {#if unfolded}
                <ChevronDown size={14} class="shrink-0 text-muted-foreground" aria-hidden="true" />
              {:else}
                <ChevronRight size={14} class="shrink-0 text-muted-foreground" aria-hidden="true" />
              {/if}
              <h3 class="text-sm font-semibold text-foreground">{$t(usageKey(group.usage))}</h3>
              <span class="text-xs tabular-nums text-muted-foreground" data-testid="catalogue-usage-count-{group.usage}">
                {group.entries.length}
              </span>
            </button>
            {#if unfolded}
              <ul class={gridClass}>
                {#each group.entries.slice(0, limitOf(group.usage)) as entry (entry.key)}{@render row(entry)}{/each}
              </ul>
              {#if group.entries.length > limitOf(group.usage)}
                <Button
                  variant="ghost"
                  size="sm"
                  class="self-center"
                  onclick={() => showMore(group.usage)}
                  data-testid="catalogue-more-hint"
                >
                  {$t("connections.add_window.show_more", {
                    values: { count: Math.min(PAGE, group.entries.length - limitOf(group.usage)) },
                  })}
                </Button>
              {/if}
            {/if}
          </section>
        {/each}

        {#if browse.source === "verified"}
          <!-- The public catalogue stays apart from the selection: what a search
               found in it, then the gesture that browses it. -->
          <section class="flex flex-col gap-2 rounded-lg border border-border/60 p-3" data-testid="catalogue-group-public">
            <div class="flex items-center gap-2">
              <h3 class="text-overline text-muted-foreground">{$t("connections.add_window.browse.community_title")}</h3>
              <Badge size="sm" variant="neutral" outline>{$t("connections.add_window.browse.badge_community")}</Badge>
            </div>
            {#if view.community.length > 0}
              <ul class={gridClass}>
                {#each view.community.slice(0, limitOf("community")) as entry (entry.key)}{@render row(entry)}{/each}
              </ul>
            {/if}
            <div class="flex flex-wrap items-center justify-between gap-3">
              {#if view.community.length > 0}
                <p class="text-caption text-muted-foreground">{$t("connections.add_window.browse.community_note")}</p>
              {/if}
              <Button
                variant="outline"
                size="sm"
                disabled={loading}
                onclick={() => chooseSource("all")}
                data-testid="catalogue-explore-all"
              >
                {$t("connections.add_window.browse.explore")}
              </Button>
            </div>
          </section>
        {/if}

        {#if !emptyShown}
          <!-- The empty state offers the address itself: it is not offered twice. -->
          <section class="flex flex-col gap-2" data-testid="catalogue-group-address">
            <h3 class="text-overline text-muted-foreground">
              {$developerMode ? $t("dev.connections.catalogue_tab_custom") : $t("connections.add_window.groups.address")}
            </h3>
            <ul class={gridClass}>
              {#each address as entry (entry.key)}{@render row(entry)}{/each}
            </ul>
          </section>
        {/if}
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
            verified={isVerified(chosenEntry)}
            onadd={() => onadd(chosenEntry)}
            onmanage={() => manage(chosenEntry)}
            oncancel={onback}
          />
        {/if}
      {/snippet}
    </BrowseLayout>
  </div>
</Dialog>
