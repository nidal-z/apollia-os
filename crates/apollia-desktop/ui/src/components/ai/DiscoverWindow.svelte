<script lang="ts">
  /**
   * DiscoverWindow - the discovery window of the AI screen. It opens on the
   * Apollia selection, which stays readable without the network; typing in
   * the search field asks the recognised publishers instead. The model chosen
   * on the left shows its card in the centre, with its download options for
   * this computer and the download of the chosen one, which the downloads
   * panel of the top bar then follows. A variant already on this computer
   * opens a conversation instead.
   */
  import { untrack } from "svelte";
  import { get } from "svelte/store";
  import { locale, t } from "svelte-i18n";
  import {
    getDictationModels,
    getHardwareProfile,
    getHfModel,
    listModelCards,
    searchHfModels,
    startVariantDownload,
    type DictationModels,
    type FoundModel,
    type HardwareProfile,
    type InstalledModel,
    type ModelCardView,
    type ModelFormat,
    type ModelVariant,
    type SearchSort,
  } from "$lib/ipc/modelHub";
  import { getEmbeddingsRecommendation, type EmbeddingsRecommendation } from "$lib/ipc/embeddings";
  import { recommendModels, startModelDownload, type SelectionFamily } from "$lib/ipc/models";
  import { createChatSession } from "$lib/ipc/agents";
  import { startRequest } from "$lib/chat/applyTargetChoice";
  import { startAgentChat } from "$lib/agents/startAgentChat";
  import { pendingChatSessionId } from "$lib/stores/chat";
  import { navigateTo } from "$lib/stores/navigation";
  import { openExternalUrl } from "$lib/utils/externalLink";
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
    dictationEntries,
    dictationVariant,
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
  import { conversationAction, readmeRepo, sourcePage } from "./discoverCard";

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
  let dictation = $state<DictationModels | null>(null);
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
  let opening = $state(false);

  let searchRun = 0;
  let detailRun = 0;

  // Where the language models come from: the Apollia selection, or the
  // recognised publishers on Hugging Face. Typing a name searches the second
  // whatever the choice.
  let source = $state<"selection" | "hub">("selection");
  const searching = $derived(
    category === "language" && (source === "hub" || isSearching(query)),
  );
  const offline = $derived(browserOffline || unreachable);
  const capacityGb = $derived(hardware?.total_capacity_gb ?? null);
  const formats = $derived(formatsFor(hardware?.accelerator));

  const entries = $derived.by((): DiscoverEntry[] => {
    if (category === "embeddings") return embeddingEntries(embeddings, cards);
    if (category === "dictation") return dictationEntries(dictation, cards);
    if (searching) return searchEntries(found, lang, cards);
    return shownSelection(selectionEntries(cards, families), { fitsOnly, capacityGb, sort });
  });

  const selected = $derived(entries.find((e) => e.key === selectedKey) ?? null);

  const variants = $derived.by((): ModelVariant[] => {
    if (!selected) return [];
    if (selected.embedding) {
      const model = selected.embedding;
      return [embeddingVariant(model, installed.some((m) => m.embedding_entry === model.id))];
    }
    if (selected.dictation.length > 0) {
      const recommended = dictation?.recommended ?? null;
      return selected.dictation.map((model) => dictationVariant(model, recommended, installed));
    }
    return detail?.variants ?? [];
  });

  const chosen = $derived(chosenVariant(variants, pickedKey));
  const action = $derived(downloadAction(chosen, { offline, started }));
  const conversation = $derived(
    selected
      ? conversationAction(selected, chosen, installed)
      : ({ kind: "blocked", reasonKey: "ai.discover.conversation.reason.not_downloaded" } as const),
  );
  const sourceUrl = $derived(selected ? sourcePage(selected, chosen) : null);
  const readmeFrom = $derived(selected ? readmeRepo(selected, chosen) : null);

  async function loadSelection(): Promise<void> {
    selectionLoading = true;
    const [catalogue, recommendation, registry, profile, voice] = await Promise.allSettled([
      listModelCards(lang),
      recommendModels(),
      getEmbeddingsRecommendation(null),
      getHardwareProfile(),
      getDictationModels(),
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
    if (voice.status === "fulfilled") dictation = voice.value;
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
    if (entry.embedding || entry.dictation.length > 0) {
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
      if (selected?.category === "dictation") {
        // A dictation file is one file at a pinned address, checked on arrival
        // against the digest of the registry.
        await startModelDownload({
          url: `https://huggingface.co/${variant.repo_id}/resolve/main/${variant.origin_file}`,
          filename: variant.origin_file,
          repo_id: variant.repo_id,
        });
      } else {
        await startVariantDownload({ repo_id: variant.repo_id, selector: variantSelector(variant) });
      }
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

  async function openConversation(): Promise<void> {
    if (conversation.kind !== "ready" || opening) return;
    const route = conversation.route;
    opening = true;
    const opened = await startAgentChat({
      createSession: () =>
        createChatSession(
          startRequest({
            kind: "model",
            route,
            label: route,
            source: "local",
            format: chosen?.format ?? null,
            server: null,
            parameters: null,
            path: null,
          }),
        ),
      rememberSession: (sessionId) => pendingChatSessionId.set(sessionId),
      report: (err) => reportError(err, { surface: "toast" }),
      navigate: () => navigateTo("chat"),
    });
    opening = false;
    if (opened) onclose();
  }

  function openSource(): void {
    if (sourceUrl) void openExternalUrl(sourceUrl);
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
      source = "selection";
      void loadSelection();
    });
  });

  // The search follows the words, the format, the sort and the box, after a pause.
  $effect(() => {
    void [query, format, sort, fitsOnly, source];
    if (!open || !searching) return;
    const timer = setTimeout(() => void runSearch(), SEARCH_DELAY_MS);
    return () => clearTimeout(timer);
  });

  // The gallery is shown until a card is asked for; a choice that left the
  // list (another usage, another search) gives the gallery back.
  $effect(() => {
    if (selectedKey !== null && !entries.some((e) => e.key === selectedKey)) selectedKey = null;
  });

  // A change of format asks the repositories of the chosen entry again.
  $effect(() => {
    void format;
    untrack(() => {
      if (open && selected && !selected.found && !selected.embedding && selected.dictation.length === 0) {
        void loadDetail(selected);
      }
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
  <div class="flex h-[min(720px,84vh)] min-h-0">
    {#if selected}
      <DiscoverCard
        entry={selected}
        {detail}
        {detailLoading}
        {format}
        {variants}
        {chosen}
        {action}
        {conversation}
        {opening}
        {sourceUrl}
        readmeRepo={readmeFrom}
        onpick={(variant) => (pickedKey = variantKey(variant))}
        ondefault={() => (pickedKey = null)}
        onsource={openSource}
        onconversation={() => void openConversation()}
        ondownload={requestDownload}
        oncopy={copyIdentifier}
        onback={() => (selectedKey = null)}
        onclose={close}
      />
    {:else}
      <DiscoverColumn
        bind:query
        bind:format
        bind:sort
        bind:fitsOnly
        bind:source
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
        onclose={close}
      />
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
