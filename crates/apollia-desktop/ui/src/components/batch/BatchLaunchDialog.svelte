<script lang="ts">
  /**
   * BatchLaunchDialog - the one dialog a batch is launched from, whether it
   * is opened from the sheet of an agent or on a file joined to the chat.
   *
   * It asks for the source (a workbook or CSV file, a folder, a mailbox, as
   * the registry of sources lists them, or a file picked on the spot), the
   * sensitive accesses granted for the duration of the batch, the number of
   * failures that interrupts it, and where the results of a file go: a copy
   * by default, the original on explicit request. The decisions live in
   * `launchBatch.ts`; this component holds the form and the calls.
   */
  import { t } from "svelte-i18n";
  import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
  import { FolderOpen, KeyRound, Layers, Mail, Table2 } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { Dialog, DialogFooter } from "$lib/components/ui/dialog";
  import { Input } from "$lib/components/ui/input";
  import { RadioGroup, RadioItem } from "$lib/components/ui/radio";
  import { Select } from "$lib/components/ui/select";
  import { addToast } from "$lib/components/ui/toast/store";
  import {
    launchBatch,
    listBatchAccesses,
    listBatchSources,
    type BatchAccess,
    type LaunchedBatch,
    type RegistrySource,
    type WriteDestination,
  } from "$lib/ipc/batches";
  import { agents } from "$lib/stores/sse";
  import {
    BLOCK_KEYS,
    DEFAULT_THRESHOLD,
    FAILURE_THRESHOLDS,
    FAMILY_KEYS,
    NEVER_INTERRUPT,
    REFUSAL_KEYS,
    SOURCE_FAMILIES,
    TABLE_EXTENSIONS,
    accessFamilyKey,
    buildLaunch,
    canWriteOriginal,
    familyChoices,
    fileChoice,
    refusalOf,
    sourceChoices,
    splitAccesses,
    writesResults,
    type LaunchForm,
    type SourceChoice,
    type SourceFamily,
  } from "./launchBatch";

  interface Props {
    open: boolean;
    onclose: () => void;
    /** The agent the batch runs; `null` lets the dialog ask for it. */
    agent: string | null;
    /** A file the dialog opens on: the one joined to the chat. */
    file?: string | null;
    onlaunched?: (batch: LaunchedBatch) => void;
  }

  let { open, onclose, agent, file = null, onlaunched }: Props = $props();

  const FAMILY_ICONS = { table: Table2, folder: FolderOpen, mail: Mail } as const;

  let chosenAgent = $state<string | null>(null);
  let registry = $state<RegistrySource[]>([]);
  let sourcesError = $state<string | null>(null);
  let picked = $state<string | null>(null);
  let family = $state<SourceFamily | null>(null);
  let source = $state<SourceChoice | null>(null);
  let accesses = $state<BatchAccess[]>([]);
  let accessesError = $state<string | null>(null);
  let granted = $state<string[]>([]);
  let threshold = $state(DEFAULT_THRESHOLD);
  let destination = $state<WriteDestination>("copy");
  let columns = $state("");
  let unreadOnly = $state(false);
  let launching = $state(false);
  let error = $state<string | null>(null);

  function message(err: unknown): string {
    return err instanceof Error ? err.message : String(err);
  }

  // A fresh form at every opening, on the file the dialog was opened with.
  $effect(() => {
    if (!open) return;
    chosenAgent = agent;
    picked = file;
    source = file ? fileChoice(file) : null;
    family = file ? "table" : null;
    granted = [];
    threshold = DEFAULT_THRESHOLD;
    destination = "copy";
    columns = $t("batch_launch.results.default_column");
    unreadOnly = false;
    error = null;
    sourcesError = null;
    let live = true;
    void listBatchSources()
      .then((found) => {
        if (live) registry = found;
      })
      .catch((err: unknown) => {
        if (!live) return;
        registry = [];
        sourcesError = message(err);
      });
    return () => {
      live = false;
    };
  });

  // The sensitive accesses are those of the agent the batch runs.
  $effect(() => {
    const name = open ? chosenAgent : null;
    accesses = [];
    accessesError = null;
    granted = [];
    if (!name) return;
    let live = true;
    void listBatchAccesses(name)
      .then((found) => {
        if (live) accesses = found;
      })
      .catch((err: unknown) => {
        if (live) accessesError = message(err);
      });
    return () => {
      live = false;
    };
  });

  const choices = $derived(sourceChoices(registry, picked));
  const listed = $derived(family ? familyChoices(choices, family) : []);
  const split = $derived(splitAccesses(accesses));
  const original = $derived(canWriteOriginal(source));
  const form = $derived<LaunchForm>({
    agent: chosenAgent,
    source,
    granted,
    threshold,
    destination,
    columns,
    unreadOnly,
  });
  const refusal = $derived(refusalOf(form));
  const agentTitle = $derived(agent ?? chosenAgent ?? "");

  function chooseFamily(next: string): void {
    if (next === family) return;
    family = next as SourceFamily;
    source = null;
    destination = "copy";
  }

  function chooseSource(key: string): void {
    source = listed.find((choice) => choice.key === key) ?? null;
    destination = "copy";
  }

  function changeSource(): void {
    source = null;
    destination = "copy";
  }

  async function pickFile(): Promise<void> {
    try {
      const path = await openFileDialog({
        multiple: false,
        filters: [{ name: $t("batch_launch.source.file_filter"), extensions: TABLE_EXTENSIONS }],
      });
      if (typeof path !== "string" || !path) return;
      picked = path;
      family = "table";
      source = fileChoice(path);
      destination = "copy";
    } catch (err) {
      error = message(err);
    }
  }

  function toggleAccess(id: string, on: boolean): void {
    granted = on ? [...granted.filter((g) => g !== id), id] : granted.filter((g) => g !== id);
  }

  async function launch(): Promise<void> {
    const body = buildLaunch(form, accesses);
    if (body === null) return;
    launching = true;
    error = null;
    try {
      const batch = await launchBatch(body);
      addToast($t("batch_launch.launched", { values: { count: batch.total } }), "success");
      onlaunched?.(batch);
      onclose();
    } catch (err) {
      error = message(err);
    } finally {
      launching = false;
    }
  }
</script>

{#snippet sourceRow(choice: SourceChoice)}
  <span class="block min-w-0">
    <span class="block truncate text-body-xs font-medium text-foreground">{choice.label}</span>
    {#if choice.detail && choice.detail !== choice.label}
      <span class="block truncate text-caption text-muted-foreground" title={choice.detail}>{choice.detail}</span>
    {/if}
    {#if choice.blocked}
      <span class="block text-caption text-warning-a11y" data-testid="batch-launch-source-blocked">
        {$t(BLOCK_KEYS[choice.blocked])}
      </span>
    {/if}
  </span>
{/snippet}

<Dialog {open} {onclose} size="xl" label={$t("batch_launch.title", { values: { name: agentTitle } })} data-testid="batch-launch-dialog">
  <div class="mb-5 flex items-start gap-3">
    <span class="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-muted text-primary">
      <Layers size={16} />
    </span>
    <div class="min-w-0">
      <h2 class="m-0 text-base font-semibold text-foreground" data-testid="batch-launch-title">
        {agentTitle
          ? $t("batch_launch.title", { values: { name: agentTitle } })
          : $t("batch_launch.title_no_agent")}
      </h2>
      <p class="m-0 text-caption text-muted-foreground">{$t("batch_launch.legend")}</p>
    </div>
  </div>

  <div class="grid gap-6 md:grid-cols-2">
    <div class="min-w-0 space-y-4">
      {#if agent === null}
        <section class="space-y-1.5">
          <h3 class="m-0 text-body-xs font-medium text-muted-foreground">{$t("batch_launch.agent.title")}</h3>
          <Select
            value={chosenAgent ?? ""}
            onchange={(event) => (chosenAgent = event.currentTarget.value || null)}
            aria-label={$t("batch_launch.agent.title")}
            data-testid="batch-launch-agent"
          >
            <option value="">{$t("batch_launch.agent.choose")}</option>
            {#each $agents as candidate (candidate.name)}
              <option value={candidate.name}>{candidate.name}</option>
            {/each}
          </Select>
        </section>
      {/if}

      <section class="space-y-1.5" data-testid="batch-launch-source" data-missing={refusal === "source_missing"}>
        <h3 class="m-0 text-body-xs font-medium text-muted-foreground">{$t("batch_launch.source.title")}</h3>
        {#if source}
          {@const Icon = FAMILY_ICONS[source.shape]}
          <div class="flex items-start gap-2 rounded-md border border-primary bg-card p-3" data-testid="batch-launch-source-chosen">
            <span class="flex h-6 w-6 shrink-0 items-center justify-center rounded bg-muted text-muted-foreground">
              <Icon size={14} />
            </span>
            {@render sourceRow(source)}
          </div>
          <Button variant="ghost" size="sm" onclick={changeSource} data-testid="batch-launch-source-change">
            {$t("batch_launch.source.change")}
          </Button>
        {:else}
          <RadioGroup value={family ?? ""}>
            {#each SOURCE_FAMILIES as kind (kind)}
              {@const Icon = FAMILY_ICONS[kind]}
              <RadioItem
                value={kind}
                checked={family === kind}
                onchange={chooseFamily}
                class={family === kind
                  ? "rounded-md border border-primary bg-card p-3"
                  : refusal === "source_missing"
                    ? "rounded-md border border-destructive/60 bg-card p-3"
                    : "rounded-md border border-border bg-card p-3"}
                data-testid={`batch-launch-family-${kind}`}
              >
                <span class="flex items-center gap-2">
                  <span class="flex h-6 w-6 shrink-0 items-center justify-center rounded bg-muted text-muted-foreground">
                    <Icon size={14} />
                  </span>
                  <span>
                    <span class="block text-body-xs font-medium text-foreground">{$t(FAMILY_KEYS[kind].name)}</span>
                    <span class="block text-caption text-muted-foreground">{$t(FAMILY_KEYS[kind].item)}</span>
                  </span>
                </span>
              </RadioItem>
            {/each}
          </RadioGroup>
          {#if family}
            <div class="space-y-1.5 pl-1" data-testid="batch-launch-registry">
              {#each listed as choice (choice.key)}
                <RadioItem
                  value={choice.key}
                  checked={false}
                  disabled={choice.reader === null}
                  onchange={chooseSource}
                  data-testid="batch-launch-source-option"
                >
                  {@render sourceRow(choice)}
                </RadioItem>
              {:else}
                <p class="m-0 text-caption text-muted-foreground" data-testid="batch-launch-registry-empty">
                  {$t(FAMILY_KEYS[family].none)}
                </p>
              {/each}
              {#if family === "table"}
                <Button variant="outline" size="sm" onclick={() => void pickFile()} data-testid="batch-launch-pick-file">
                  {$t("batch_launch.source.pick_file")}
                </Button>
              {/if}
            </div>
          {/if}
          {#if sourcesError}
            <p class="m-0 text-caption text-destructive" data-testid="batch-launch-sources-error">
              {$t("batch_launch.source.unreadable", { values: { reason: sourcesError } })}
            </p>
          {/if}
        {/if}
        {#if source?.shape === "mail"}
          <label class="flex cursor-pointer select-none items-center gap-2 text-body-xs text-foreground">
            <Checkbox bind:checked={unreadOnly} data-testid="batch-launch-unread-only" />
            {$t("batch_launch.source.unread_only")}
          </label>
        {/if}
      </section>

      {#if writesResults(source)}
        <section class="space-y-1.5" data-testid="batch-launch-results">
          <h3 class="m-0 text-body-xs font-medium text-muted-foreground">{$t("batch_launch.results.title")}</h3>
          <RadioGroup value={destination}>
            <RadioItem
              value="copy"
              checked={destination === "copy"}
              onchange={() => (destination = "copy")}
              data-testid="batch-launch-destination-copy"
            >
              {$t("batch_launch.results.copy")}
            </RadioItem>
            <RadioItem
              value="original"
              checked={destination === "original"}
              disabled={!original}
              onchange={() => (destination = "original")}
              data-testid="batch-launch-destination-original"
            >
              {$t("batch_launch.results.original")}
            </RadioItem>
          </RadioGroup>
          {#if !original}
            <p class="m-0 text-caption text-muted-foreground" data-testid="batch-launch-original-unavailable">
              {$t("batch_launch.results.original_unavailable")}
            </p>
          {/if}
          <div class="space-y-1 pt-1">
            <div class="text-body-xs font-medium text-foreground">{$t("batch_launch.results.columns")}</div>
            <Input
              bind:value={columns}
              aria-label={$t("batch_launch.results.columns")}
              data-testid="batch-launch-columns"
            />
            <p class="m-0 text-caption text-muted-foreground">{$t("batch_launch.results.columns_hint")}</p>
          </div>
        </section>
      {/if}
    </div>

    <div class="min-w-0 space-y-4">
      <section class="space-y-1.5" data-testid="batch-launch-accesses">
        <h3 class="m-0 text-body-xs font-medium text-muted-foreground">{$t("batch_launch.accesses.title")}</h3>
        {#if accessesError}
          <p class="m-0 text-caption text-destructive" data-testid="batch-launch-accesses-error">{accessesError}</p>
        {:else if accesses.length === 0}
          <p class="m-0 text-caption text-muted-foreground" data-testid="batch-launch-accesses-none">
            {chosenAgent ? $t("batch_launch.accesses.none") : $t("batch_launch.accesses.no_agent")}
          </p>
        {:else}
          <div class="space-y-3 rounded-md border border-border bg-card p-3">
            {#each split.grantable as access (access.accessId)}
              <label class="flex cursor-pointer select-none items-start gap-2" data-testid="batch-launch-access">
                <Checkbox
                  checked={granted.includes(access.accessId)}
                  onchange={(on) => toggleAccess(access.accessId, on)}
                  data-testid="batch-launch-access-grant"
                />
                <KeyRound size={14} class="mt-0.5 shrink-0 text-muted-foreground" />
                <span class="min-w-0">
                  <span class="block text-body-xs font-medium text-foreground">{access.permits}</span>
                  <span class="block text-caption text-muted-foreground">{$t(accessFamilyKey(access.family))}</span>
                </span>
              </label>
            {/each}
            {#each split.asked as access (access.accessId)}
              <div class="flex items-start gap-2" data-testid="batch-launch-access-asked">
                <KeyRound size={14} class="mt-0.5 shrink-0 text-muted-foreground" />
                <span class="min-w-0">
                  <span class="block text-body-xs font-medium text-foreground">{access.permits}</span>
                  <span class="block text-caption text-muted-foreground">{$t("batch_launch.accesses.asked_each_time")}</span>
                </span>
              </div>
            {/each}
          </div>
          <p class="m-0 text-caption text-muted-foreground">{$t("batch_launch.accesses.rule")}</p>
        {/if}
      </section>

      <section class="space-y-1.5">
        <h3 class="m-0 text-body-xs font-medium text-muted-foreground">{$t("batch_launch.threshold.title")}</h3>
        <div class="flex items-center gap-2">
          <span class="text-body-xs font-medium text-foreground">{$t("batch_launch.threshold.label")}</span>
          <div class="w-52">
            <Select
              bind:value={threshold}
              size="sm"
              aria-label={$t("batch_launch.threshold.label")}
              data-testid="batch-launch-threshold"
            >
              {#each FAILURE_THRESHOLDS as count (count)}
                <option value={String(count)}>{$t("batch_launch.threshold.failures", { values: { count } })}</option>
              {/each}
              <option value={NEVER_INTERRUPT}>{$t("batch_launch.threshold.never")}</option>
            </Select>
          </div>
        </div>
        <p class="m-0 text-caption text-muted-foreground">{$t("batch_launch.threshold.hint")}</p>
      </section>
    </div>
  </div>

  {#if error}
    <p class="m-0 mt-4 text-caption text-destructive" data-testid="batch-launch-error">{error}</p>
  {/if}
  <DialogFooter class="items-center sm:justify-between">
    <p
      class={refusal ? "m-0 text-caption text-destructive" : "m-0 text-caption text-muted-foreground"}
      data-testid="batch-launch-refusal"
      data-refusal={refusal ?? "none"}
    >
      {refusal ? $t(REFUSAL_KEYS[refusal]) : $t("batch_launch.ready")}
    </p>
    <span class="flex gap-2">
      <Button variant="ghost" onclick={onclose} data-testid="batch-launch-cancel">{$t("common.cancel")}</Button>
      <Button
        variant="primary-solid"
        disabled={refusal !== null || launching}
        onclick={() => void launch()}
        data-testid="batch-launch-confirm"
      >
        {launching ? $t("batch_launch.launching") : $t("batch_launch.confirm")}
      </Button>
    </span>
  </DialogFooter>
</Dialog>
