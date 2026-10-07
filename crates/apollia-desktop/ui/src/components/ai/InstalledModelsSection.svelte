<script lang="ts">
  /**
   * InstalledModelsSection - the "On this computer" section of the AI screen:
   * the filter, the table of the models of the chosen category, and the
   * footer with their count, their size and the models folder. A click on
   * a row, or on its gear, opens the panel of that model.
   */
  import { locale, t } from "svelte-i18n";
  import { ArrowUpToLine, Download, FolderOpen, Search, Settings, Upload } from "lucide-svelte";
  import type { InstalledModel } from "$lib/ipc/modelHub";
  import { cn, formatRelativeTime } from "$lib/utils";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Tooltip } from "$lib/components/ui/tooltip";
  import CapabilityIcon from "./CapabilityIcon.svelte";
  import ExplainedBadge from "./ExplainedBadge.svelte";
  import ModelActionsMenu from "./ModelActionsMenu.svelte";
  import { modelFullFileName } from "$lib/stores/modelFullFileName";
  import { capabilitiesOf } from "./explanations";
  import {
    categoryLabelKey,
    emptyCategoryKey,
    filterShortcutLabel,
    gigabytes,
    groupModels,
    parameterCount,
    revealLabelKey,
    shownModelName,
    totalBytes,
    type CategoryFilter,
    type HostPlatform,
    type MenuEntryId,
  } from "./aiModels";

  interface Props {
    models: InstalledModel[];
    /** Paths of the models resident in memory. */
    loadedPaths: ReadonlySet<string>;
    filter: CategoryFilter;
    query: string;
    platform: HostPlatform;
    modelsDir: string | null;
    error: string | null;
    importing: boolean;
    /** Identifier of the model whose panel is open, if any. */
    openId: string | null;
    onselect: (model: InstalledModel, entry: MenuEntryId) => void;
    /** Opens the panel of a model. */
    onopen: (model: InstalledModel) => void;
    onimport: () => void;
    ondiscover: () => void;
    onrevealfolder: () => void;
    onretry: () => void;
  }

  let {
    models,
    loadedPaths,
    filter,
    query = $bindable(""),
    platform,
    modelsDir,
    error,
    importing,
    openId,
    onselect,
    onopen,
    onimport,
    ondiscover,
    onrevealfolder,
    onretry,
  }: Props = $props();

  const lang = $derived($locale ?? "fr");
  const groups = $derived(groupModels(models, filter, query));
  const shown = $derived(groups.reduce((n, g) => n + g.models.length, 0));

  /** A click on a row opens its panel, unless it landed on a control of the row. */
  function openFromRow(event: MouseEvent, model: InstalledModel): void {
    const target = event.target;
    if (target instanceof Element && target.closest("button, a, [role='menu']")) return;
    onopen(model);
  }
</script>

<section class="flex min-h-0 flex-1 flex-col gap-3" data-testid="ai-installed-section">
  <div class="flex items-center gap-2">
    <h2 class="mr-auto shrink-0 text-sm font-semibold text-foreground">{$t("ai.installed.title")}</h2>
    <div class="w-56 min-w-28 shrink">
      <Input
        size="sm"
        icon={Search}
        bind:value={query}
        placeholder={$t("ai.installed.filter_placeholder")}
        aria-label={$t("ai.installed.filter_label")}
        data-testid="ai-filter-input"
      >
        {#snippet trailing()}
          <kbd class="text-overline text-muted-foreground" data-testid="ai-filter-shortcut">
            {filterShortcutLabel(platform)}
          </kbd>
        {/snippet}
      </Input>
    </div>
    <Button variant="outline" size="sm" onclick={onimport} disabled={importing} data-testid="ai-import-btn">
      <Upload size={14} aria-hidden="true" />
      {$t("ai.installed.import")}
    </Button>
    <Button variant="primary-solid" size="sm" onclick={ondiscover} data-testid="ai-discover-btn">
      <Download size={14} aria-hidden="true" />
      {$t("ai.installed.discover")}
    </Button>
  </div>

  {#if error}
    <div
      class="flex items-center justify-between gap-3 rounded-lg border border-destructive/30 bg-destructive/5 p-3 text-sm"
      data-testid="ai-installed-error"
    >
      <span class="text-foreground">{$t("ai.installed.error")}</span>
      <Button variant="outline" size="sm" onclick={onretry} data-testid="ai-installed-retry">
        {$t("common.retry")}
      </Button>
    </div>
  {:else if models.length === 0}
    <div
      class="flex flex-col items-center gap-3 rounded-lg border border-dashed border-border p-8 text-center"
      data-testid="ai-installed-empty"
    >
      <p class="text-sm font-medium text-foreground">{$t("ai.installed.empty_title")}</p>
      <p class="max-w-md text-sm text-muted-foreground">{$t("ai.installed.empty_hint")}</p>
      <div class="flex gap-2">
        <Button variant="primary-solid" size="sm" onclick={ondiscover} data-testid="ai-empty-discover-btn">
          {$t("ai.installed.discover")}
        </Button>
        <Button variant="outline" size="sm" onclick={onimport} data-testid="ai-empty-import-btn">
          {$t("ai.installed.import")}
        </Button>
      </div>
    </div>
  {:else}
    <div class="min-h-0 flex-1 overflow-auto" data-testid="ai-installed-table">
      {#each groups as group (group.category)}
        {#if filter === "all"}
          <h3 class="mb-2 mt-4 text-overline uppercase text-muted-foreground first:mt-0" data-testid="ai-group-{group.category}">
            {$t("ai.installed.group", {
              values: { label: $t(categoryLabelKey(group.category)), count: group.models.length },
            })}
          </h3>
        {/if}
        {#if group.models.length === 0}
          <p class="py-6 text-center text-sm text-muted-foreground" data-testid="ai-category-empty">
            {query.trim()
              ? $t("ai.installed.no_match", { values: { query: query.trim() } })
              : $t(emptyCategoryKey(group.category))}
          </p>
        {/if}
        <ul class="grid grid-cols-1 gap-3 lg:grid-cols-2 2xl:grid-cols-3">
          {#each group.models as model (model.path)}
            {@const params = parameterCount(model.parameters, lang)}
            <li class="min-w-0">
              <!-- The gear of the card is the keyboard path to the same panel. -->
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div
                class={cn(
                  "flex h-full cursor-pointer flex-col gap-2.5 rounded-xl border bg-surface-1 p-4 transition-colors",
                  openId === model.id
                    ? "border-primary/50 bg-primary/5"
                    : "border-border hover:border-primary/30 hover:bg-surface-2",
                )}
                aria-current={openId === model.id ? "true" : undefined}
                onclick={(event) => openFromRow(event, model)}
                data-testid="ai-model-row"
                data-model-id={model.id}
              >
                <div class="flex items-start gap-2">
                  <div class="flex min-w-0 flex-1 flex-wrap items-center gap-1.5">
                    {#if model.pinned_at}
                      <Tooltip content={$t("ai.installed.pinned_explanation")}>
                        <span class="inline-flex text-primary" role="img" aria-label={$t("ai.menu.pin")} data-testid="ai-model-pinned">
                          <ArrowUpToLine size={13} aria-hidden="true" />
                        </span>
                      </Tooltip>
                    {/if}
                    <span class="min-w-0 truncate text-sm font-semibold text-foreground" data-testid="ai-model-name">{shownModelName(model, $modelFullFileName)}</span>
                    {#each capabilitiesOf(model.capabilities) as capability (capability)}
                      <CapabilityIcon {capability} />
                    {/each}
                  </div>
                  <div class="-mr-1.5 -mt-1 inline-flex shrink-0 items-center gap-0.5">
                    <Tooltip
                      content={$t("ai.panel.open")}
                      onclick={() => onopen(model)}
                      triggerLabel={$t("ai.panel.open_named", { values: { name: model.id } })}
                      triggerTestid="ai-model-settings-btn"
                      triggerClass="h-7 w-7 justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
                    >
                      <Settings size={15} strokeWidth={1.75} aria-hidden="true" />
                    </Tooltip>
                    <ModelActionsMenu {model} {platform} {onselect} />
                  </div>
                </div>
                <div class="flex flex-wrap items-center gap-1.5">
                  {#if loadedPaths.has(model.path)}
                    <ExplainedBadge
                      label={$t("ai.badge.loaded")}
                      explanation={$t("ai.badge.loaded_explanation")}
                      variant="success"
                      testid="ai-model-loaded"
                    />
                  {/if}
                  {#if model.default_chat}
                    <ExplainedBadge
                      label={$t("ai.badge.default")}
                      explanation={$t("ai.badge.default_explanation")}
                      variant="primary"
                      testid="ai-model-default"
                    />
                  {/if}
                  {#if model.default_search}
                    <ExplainedBadge
                      label={$t("ai.badge.search")}
                      explanation={$t("ai.badge.search_explanation")}
                      variant="primary"
                      testid="ai-model-search"
                    />
                  {/if}
                  <ExplainedBadge
                    label={model.format.toUpperCase()}
                    explanation={$t("ai.format_explanation")}
                    help
                    testid="ai-model-format"
                  />
                  {#if model.parameters}
                    <ExplainedBadge
                      label={params ? $t(params.unitKey, { values: { n: params.value } }) : model.parameters}
                      explanation={$t("ai.installed.params_explanation")}
                      testid="ai-model-params"
                    />
                  {/if}
                  {#if !model.origin_verified}
                    <ExplainedBadge
                      label={$t("ai.badge.unverified")}
                      explanation={$t("ai.badge.unverified_explanation")}
                      variant="warning"
                      testid="ai-model-unverified"
                    />
                  {/if}
                </div>
                <p class="mt-auto flex items-baseline gap-2 text-xs text-muted-foreground">
                  <span class="shrink-0 tabular-nums text-foreground">
                    {$t("ai.installed.size", { values: { size: gigabytes(model.size_bytes, lang) } })}
                  </span>
                  {#if model.used_by}
                    <span class="min-w-0 flex-1 truncate" data-testid="ai-model-used-by">
                      {$t("ai.installed.used_by", { values: { name: model.used_by } })}
                    </span>
                  {:else}
                    <span class="flex-1"></span>
                  {/if}
                  <span class="shrink-0">{model.modified_at ? formatRelativeTime(model.modified_at, lang) : ""}</span>
                </p>
              </div>
            </li>
          {/each}
        </ul>
      {/each}
      {#if filter === "all" && shown === 0 && query.trim()}
        <p class="py-6 text-center text-sm text-muted-foreground" data-testid="ai-category-empty">
          {$t("ai.installed.no_match", { values: { query: query.trim() } })}
        </p>
      {/if}
    </div>
  {/if}

  <footer class="flex flex-wrap items-center justify-between gap-3 border-t border-border pt-3" data-testid="ai-installed-footer">
    <span class="text-sm text-foreground" data-testid="ai-installed-total">
      {$t("ai.installed.footer", {
        values: { count: models.length, size: gigabytes(totalBytes(models), lang) },
      })}
    </span>
    {#if modelsDir}
      <div class="flex items-center gap-1.5">
        <code
          class="rounded border border-border px-2 py-0.5 text-xs text-muted-foreground"
          aria-label={$t("ai.installed.folder_label")}
          data-testid="ai-models-dir"
        >{modelsDir}</code>
        <Tooltip
          content={$t(revealLabelKey(platform))}
          onclick={onrevealfolder}
          triggerLabel={$t(revealLabelKey(platform))}
          triggerTestid="ai-reveal-folder-btn"
          triggerClass="h-7 w-7 justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
        >
          <FolderOpen size={14} aria-hidden="true" />
        </Tooltip>
      </div>
    {/if}
  </footer>
</section>
