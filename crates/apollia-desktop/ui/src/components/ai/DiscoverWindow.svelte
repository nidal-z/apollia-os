<script lang="ts">
  /**
   * DiscoverWindow - the discovery window of the AI screen. It opens on the
   * Apollia selection, which stays readable without the network; typing in
   * the search field asks the recognised publishers instead. The model chosen
   * on the left shows its card in the centre, with its variants for this
   * computer and the download of the chosen one, which the downloads panel of
   * the top bar then follows.
   */
  import { untrack } from "svelte";
  import { get } from "svelte/store";
  import { locale, t } from "svelte-i18n";
  import {
    getHardwareProfile,
    getHfModel,
    listModelCards,
    searchHfModels,
    startVariantDownload,
    type FoundModel,
    type HardwareProfile,
    type InstalledModel,
    type ModelCardView,
    type ModelFormat,
    type ModelVariant,
    type SearchSort,
  } from "$lib/ipc/modelHub";
  import { getEmbeddingsRecommendation, type EmbeddingsRecommendation } from "$lib/ipc/embeddings";
  import { recommendModels, type SelectionFamily } from "$lib/ipc/models";
  import { getHfToken } from "$lib/ipc/modelsFolder";
  import { downloads } from "$lib/stores/downloads";
  import { openSettingsWindow } from "$lib/stores/settingsWindow";
  import { reportError } from "$lib/errors/reportError";
  import { addToast } from "$lib/components/ui/toast";
  import { Dialog } from "$lib/components/ui/dialog";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import DiscoverColumn from "./DiscoverColumn.svelte";
  import DiscoverCard from "./DiscoverCard.svelte";
  import { gigabytes } from "./aiModels";
  import {
    chosenVariant,
    detailFromFound,
    detailFromRepos,
    downloadAction,
    embeddingEntries,
    embeddingVariant,
    formatsFor,
    isOfflineError,
    isSearching,
    searchEntries,
    selectionEntries,
    shownSelection,
    variantKey,
    variantSelector,
    type CardDetail,
    type DiscoverCategory,
    type DiscoverEntry,
  } from "./discover";

  interface Props {
    open: boolean;
    /** The models of this computer, to mark what is already here. */
    installed: readonly InstalledModel[];
    onclose: () => void;
    /** A download started here has finished: the models of the computer changed. */
    ondownloaded: () => void;
  }

  let { open, installed, onclose, ondownloaded }: Props = $props();

  /** How long the field waits for the next key before it searches. */
  const SEARCH_DELAY_MS = 350;
  /** How many repositories of one selection entry are asked for its variants. */
  const MAX_REPOS = 4;

  const lang = $derived($locale ?? "fr");

  let query = $state("");
  let category = $state<DiscoverCategory>("language");
  let format = $state<ModelFormat>("gguf");
  let sort = $state<SearchSort>("best_match");
  let fitsOnly = $state(false);

  let cards = $state<ModelCardView[]>([]);
  let selectionNote = $state<string | null>(null);
  let families = $state<SelectionFamily[]>([]);
  let embeddings = $state<EmbeddingsRecommendation | null>(null);
  let hardware = $state<HardwareProfile | null>(null);
  let selectionLoading = $state(false);

  let found = $state<FoundModel[]>([]);
  let searchLoading = $state(false);
  let searchError = $state<string | null>(null);

  /** The publishers could not be reached at the last attempt. */
  let unreachable = $state(false);
  let browserOffline = $state(typeof navigator !== "undefined" && navigator.onLine === false);

  let selectedKey = $state<string | null>(null);
  let detail = $state<CardDetail | null>(null);
  let detailLoading = $state(false);
  let pickedKey = $state<string | null>(null);
  let started = $state<Set<string>>(new Set());
  let confirming = $state<ModelVariant | null>(null);

  let searchRun = 0;
  let detailRun = 0;

  const searching = $derived(category === "language" && isSearching(query));
  const offline = $derived(browserOffline || unreachable);
  const capacityGb = $derived(hardware?.total_capacity_gb ?? null);
  const formats = $derived(formatsFor(hardware?.accelerator));

  const entries = $derived.by((): DiscoverEntry[] => {
    if (category === "embeddings") return embeddingEntries(embeddings, cards);
    if (searching) return searchEntries(found, lang);
    return shownSelection(selectionEntries(cards, families), { fitsOnly, capacityGb, sort });
  });

  const selected = $derived(entries.find((e) => e.key === selectedKey) ?? null);

  const variants = $derived.by((): ModelVariant[] => {
    if (!selected) return [];
    if (selected.embedding) {
      const model = selected.embedding;
      return [embeddingVariant(model, installed.some((m) => m.embedding_entry === model.id))];
    }
    return detail?.variants ?? [];
  });

  const chosen = $derived(chosenVariant(variants, pickedKey));
  const action = $derived(downloadAction(chosen, { offline, started }));

  async function loadSelection(): Promise<void> {
    selectionLoading = true;
    const [catalogue, recommendation, registry, profile] = await Promise.allSettled([
      listModelCards(lang),
      recommendModels(),
      getEmbeddingsRecommendation(null),
      getHardwareProfile(),
    ]);
    if (catalogue.status === "fulfilled") {
      cards = catalogue.value.cards;
      selectionNote = catalogue.value.cards.length === 0 ? catalogue.value.message : null;
    }
    if (recommendation.status === "fulfilled") {
      families = recommendation.value.selection;
      unreachable = recommendation.value.status === "unreachable";
    }
    if (registry.status === "fulfilled") embeddings = registry.value;
    if (profile.status === "fulfilled") hardware = profile.value;
    selectionLoading = false;
  }

  async function runSearch(): Promise<void> {
    const run = ++searchRun;
    searchLoading = true;
    searchError = null;
    try {
      const page = await searchHfModels({ query: query.trim(), format, sort, fits_only: fitsOnly });
      if (run !== searchRun) return;
      found = page.models;
      unreachable = false;
    } catch (err: unknown) {
      if (run !== searchRun) return;
      const message = err instanceof Error ? err.message : String(err);
      found = [];
      if (isOfflineError(message)) unreachable = true;
      else searchError = message;
    } finally {
      if (run === searchRun) searchLoading = false;
    }
  }

  async function loadDetail(entry: DiscoverEntry): Promise<void> {
    const run = ++detailRun;
    detail = null;
    if (entry.embedding) {
      detailLoading = false;
      return;
    }
    if (entry.found) {
      detail = detailFromFound(entry.found);
      detailLoading = false;
      return;
    }
    detailLoading = true;
    const answers = await Promise.allSettled(entry.repos.slice(0, MAX_REPOS).map((repo) => getHfModel(repo)));
    if (run !== detailRun) return;
    const read = answers.flatMap((a) => (a.status === "fulfilled" ? [a.value] : []));
    detail = read.length > 0 ? detailFromRepos(read, format) : null;
    if (read.length > 0) unreachable = false;
    detailLoading = false;
  }

  function select(entry: DiscoverEntry): void {
    selectedKey = entry.key;
    pickedKey = null;
    void loadDetail(entry);
  }

  function setCategory(next: DiscoverCategory): void {
    category = next;
    selectedKey = null;
  }

  async function copyIdentifier(): Promise<void> {
    if (!selected) return;
    try {
      await navigator.clipboard.writeText(selected.identifier);
      addToast(get(t)("ai.toast.copied_id"), "success");
    } catch {
      addToast(get(t)("ai.toast.copy_failed"), "error");
    }
  }

  async function download(variant: ModelVariant): Promise<void> {
    confirming = null;
    if (detail?.gated) {
      // A restricted repository answers only to the token kept in General.
      const token = await getHfToken().catch(() => null);
      if (token === null) {
        addToast(get(t)("ai.discover.token_needed"), "info");
        // The settings open above the screen: this window leaves the way.
        onclose();
        openSettingsWindow("general");
        return;
      }
    }
    const key = variantKey(variant);
    started = new Set([...started, key]);
    try {
      await startVariantDownload({ repo_id: variant.repo_id, selector: variantSelector(variant) });
      addToast(get(t)("ai.discover.download_started"), "info");
    } catch (err: unknown) {
      started = new Set([...started].filter((k) => k !== key));
      reportError(err, { surface: "toast" });
    }
  }

  function requestDownload(): void {
    if (!chosen) return;
    if (action === "confirm") confirming = chosen;
    else if (action === "download") void download(chosen);
  }

  function close(): void {
    if (confirming !== null) return;
    onclose();
  }

  // Opening the window reads the selection; closing it forgets the search.
  $effect(() => {
    if (!open) return;
    untrack(() => {
      query = "";
      found = [];
      searchError = null;
      selectedKey = null;
      void loadSelection();
    });
  });

  // The search follows the words, the format, the sort and the box, after a pause.
  $effect(() => {
    void [query, format, sort, fitsOnly];
    if (!open || !searching) return;
    const timer = setTimeout(() => void runSearch(), SEARCH_DELAY_MS);
    return () => clearTimeout(timer);
  });

  // The first row is shown when nothing is chosen, or when the choice left the list.
  $effect(() => {
    const first = entries[0];
    if (!open || !first) return;
    if (selectedKey !== null && entries.some((e) => e.key === selectedKey)) return;
    untrack(() => select(first));
  });

  // A change of format asks the repositories of the chosen entry again.
  $effect(() => {
    void format;
    untrack(() => {
      if (open && selected && !selected.found && !selected.embedding) void loadDetail(selected);
    });
  });

  // A download that finishes puts a model on this computer: the card is read again.
  let finished = 0;
  $effect(() => {
    const done = $downloads.filter((d) => d.status === "completed").length;
    if (done === finished) return;
    const grew = done > finished;
    finished = done;
    if (!grew || !open) return;
    untrack(() => {
      started = new Set();
      ondownloaded();
      if (selected) void loadDetail(selected);
    });
  });

  $effect(() => {
    const online = (): void => {
      browserOffline = false;
    };
    const offlineNow = (): void => {
      browserOffline = true;
    };
    window.addEventListener("online", online);
    window.addEventListener("offline", offlineNow);
    return () => {
      window.removeEventListener("online", online);
      window.removeEventListener("offline", offlineNow);
    };
  });
</script>

<Dialog
  {open}
  onclose={close}
  size="xl"
  label={$t("ai.discover.title")}
  bodyClass="p-0"
  hideClose
  data-testid="ai-discover-window"
>
  <div class="flex h-[min(680px,80vh)] min-h-0">
    <DiscoverColumn
      bind:query
      bind:format
      bind:sort
      bind:fitsOnly
      {category}
      {formats}
      {capacityGb}
      {entries}
      {selectedKey}
      {searching}
      loading={searching ? searchLoading : selectionLoading}
      {offline}
      {searchError}
      {selectionNote}
      onselect={select}
      oncategory={setCategory}
      onrefresh={() => void loadSelection()}
    />
    {#if selected}
      <DiscoverCard
        entry={selected}
        {detail}
        {detailLoading}
        {format}
        {variants}
        {chosen}
        {action}
        onpick={(variant) => (pickedKey = variantKey(variant))}
        ondownload={requestDownload}
        oncopy={copyIdentifier}
        onclose={close}
      />
    {:else}
      <div class="flex min-w-0 flex-1 flex-col p-5" data-testid="ai-discover-card-empty">
        <button
          type="button"
          class="ml-auto rounded-md px-2 py-1 text-sm text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
          onclick={close}
          data-testid="ai-discover-close-empty"
        >
          {$t("common.close")}
        </button>
        <p class="m-auto max-w-sm text-center text-sm text-muted-foreground">{$t("ai.discover.card_empty")}</p>
      </div>
    {/if}
  </div>
</Dialog>

<ConfirmDialog
  open={confirming !== null}
  onclose={() => (confirming = null)}
  onconfirm={() => {
    if (confirming) void download(confirming);
  }}
  title={$t("ai.discover.too_large.title")}
  message={capacityGb === null
    ? $t("ai.discover.too_large.message_unknown", {
        values: { size: confirming ? gigabytes(confirming.size_bytes, lang) : "" },
      })
    : $t("ai.discover.too_large.message", {
        values: {
          size: confirming ? gigabytes(confirming.size_bytes, lang) : "",
          capacity: gigabytes(capacityGb * 1e9, lang),
        },
      })}
  confirmLabel={$t("ai.discover.too_large.confirm")}
  data-testid="ai-discover-too-large-dialog"
/>
