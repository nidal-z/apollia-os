<script lang="ts">
  /**
   * The loading window: what a local model would take in memory before it is
   * loaded, and the settings of this load. It opens on the model
   * `modelLoadAsk` names, estimates again at each change, and loads on
   * "Load" (Cmd Enter on macOS, Ctrl Enter elsewhere).
   *
   * A load the guardrail refuses shows the estimate and the limit; "Load
   * anyway" asks for an explicit confirmation before anything is sent. A
   * model that takes no setting (MLX) shows its memory and says so.
   */
  import { untrack } from "svelte";
  import { get } from "svelte/store";
  import { locale, t } from "svelte-i18n";
  import { ArrowLeft, CircleHelp, CornerDownLeft, Pin, TriangleAlert, X } from "lucide-svelte";
  import { Dialog } from "$lib/components/ui/dialog";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import { Button } from "$lib/components/ui/button";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { Toggle } from "$lib/components/ui/toggle";
  import { Tooltip } from "$lib/components/ui/tooltip";
  import { Spinner } from "$lib/components/ui/progress";
  import { addToast } from "$lib/components/ui/toast";
  import { MOD_LABEL } from "$lib/shortcuts/chatShortcuts";
  import { estimateModelLoad, loadModel, setModelPin } from "$lib/ipc/llmLoad";
  import {
    attemptLoad,
    clampContext,
    clampOffload,
    contextBounds,
    emptyForm,
    gb,
    loadRequest,
    refusalOf,
    shownOffload,
    type LoadForm,
  } from "$lib/chat/modelLoad";
  import { modelLoadAsk, modelPins, refreshModelResidency } from "$lib/stores/modelResidency";
  import type { LoadEstimate, ModelLoadRequest } from "$lib/types";
  import ExplainedBadge from "../ai/ExplainedBadge.svelte";
  import ModelLoadAdvanced from "./ModelLoadAdvanced.svelte";

  const ask = $derived($modelLoadAsk);
  const lang = $derived($locale ?? "fr");

  let form = $state<LoadForm>(emptyForm());
  let estimate = $state<LoadEstimate | null>(null);
  let estimating = $state(false);
  let estimateError = $state<string | null>(null);
  let loading = $state(false);
  let loadError = $state<string | null>(null);
  let confirmOpen = $state(false);
  let advancedShown = $state(false);
  let pinnedBefore = false;
  let run = 0;

  const configurable = $derived(estimate ? estimate.configurable : ask?.format !== "mlx");
  const format = $derived(
    estimate ? (estimate.engine === "vllm-mlx" ? "mlx" : "gguf") : (ask?.format ?? null),
  );
  const refusal = $derived(refusalOf(estimate));
  const bounds = $derived(estimate ? contextBounds(estimate) : null);
  const context = $derived(form.nCtx ?? estimate?.nCtx ?? 0);
  const offload = $derived(estimate ? shownOffload(form, estimate) : 0);
  const layers = $derived(estimate?.totalLayers ?? null);

  // A new model: the window starts from the settings it was asked to open
  // with, else from the model's own.
  $effect(() => {
    const current = ask;
    untrack(() => {
      form = current?.settings ? { ...current.settings } : emptyForm();
      estimate = null;
      estimateError = null;
      loadError = null;
      loading = false;
      confirmOpen = false;
      advancedShown = false;
      pinnedBefore = false;
    });
  });

  // What the estimate depends on: the model and the settings, never the two
  // choices of the foot of the window.
  const estimateKey = $derived(
    ask
      ? JSON.stringify(
          loadRequest(ask.modelId, { ...form, remember: false }, ask.format !== "mlx"),
        )
      : null,
  );

  $effect(() => {
    const key = estimateKey;
    if (key === null) return;
    const mine = ++run;
    const first = untrack(() => estimate === null);
    const timer = setTimeout(
      () => void runEstimate(JSON.parse(key) as ModelLoadRequest, mine, first),
      first ? 0 : 200,
    );
    return () => clearTimeout(timer);
  });

  async function runEstimate(
    request: ModelLoadRequest,
    mine: number,
    first: boolean,
  ): Promise<void> {
    estimating = true;
    try {
      const answer = await estimateModelLoad(request);
      if (mine !== run) return;
      estimate = answer;
      estimateError = null;
      if (first) {
        pinnedBefore = get(modelPins).includes(answer.modelPath);
        form.pin = pinnedBefore;
      }
    } catch (err: unknown) {
      if (mine !== run) return;
      estimateError = err instanceof Error ? err.message : String(err);
    } finally {
      if (mine === run) estimating = false;
    }
  }

  function close(): void {
    if (loading) return;
    modelLoadAsk.set(null);
  }

  function numberOf(event: Event): number {
    return Number((event.currentTarget as HTMLInputElement).value);
  }

  function setContext(event: Event): void {
    if (estimate) form.nCtx = clampContext(numberOf(event), estimate);
  }

  function setOffload(event: Event): void {
    if (estimate) form.nGpuLayers = clampOffload(numberOf(event), estimate);
  }

  async function submit(confirmed: boolean): Promise<void> {
    const current = ask;
    if (!current || loading || estimate === null) return;
    confirmOpen = false;
    loading = true;
    loadError = null;
    const attempt = await attemptLoad(
      { load: loadModel, setPin: setModelPin },
      current.modelId,
      form,
      estimate,
      confirmed,
      pinnedBefore,
    );
    loading = false;
    if (attempt.kind === "needs_confirmation") {
      confirmOpen = true;
      return;
    }
    if (attempt.kind === "failed") {
      loadError = attempt.message;
      return;
    }
    await refreshModelResidency();
    addToast($t("chat.model_load.loaded_toast", { values: { name: current.modelId } }), "success");
    modelLoadAsk.set(null);
    current.onloaded?.();
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (!ask || confirmOpen || event.key !== "Enter") return;
    if (!(event.metaKey || event.ctrlKey)) return;
    event.preventDefault();
    void submit(false);
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<Dialog
  open={ask !== null}
  onclose={close}
  size="md"
  label={$t("chat.model_load.aria")}
  bodyClass="p-5"
  hideClose
  data-testid="model-load-dialog"
>
  {#if ask}
    <div class="flex items-center gap-2">
      <button
        type="button"
        class="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-surface-2 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
        onclick={close}
        aria-label={$t("chat.model_load.back")}
        data-testid="model-load-back"
      >
        <ArrowLeft size={15} strokeWidth={1.75} />
      </button>
      <h2 class="min-w-0 flex-1 truncate text-base font-semibold text-foreground" data-testid="model-load-title">
        {ask.modelId}
      </h2>
      <button
        type="button"
        class="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-surface-2 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
        onclick={close}
        aria-label={$t("common.close")}
        data-testid="model-load-close"
      >
        <X size={15} strokeWidth={1.75} />
      </button>
    </div>

    <div class="mt-4 space-y-4">
      <div
        class="flex flex-wrap items-center gap-x-4 gap-y-1 rounded-lg bg-muted px-3 py-2 text-body-xs"
        aria-live="polite"
        data-testid="model-load-memory"
      >
        <span class="font-medium text-foreground">{$t("chat.model_load.memory")}</span>
        <span class="flex-1 text-caption text-muted-foreground">
          {estimating ? $t("chat.model_load.estimating") : $t("chat.model_load.estimate_mark")}
        </span>
        {#if estimate?.memory}
          <span class="text-caption text-muted-foreground">{$t("chat.model_load.memory_gpu")}</span>
          <span class="font-medium text-foreground" data-testid="model-load-memory-gpu">
            {$t("chat.model_load.gigabytes", { values: { size: gb(estimate.memory.gpuGb, lang) } })}
          </span>
          <span class="text-caption text-muted-foreground">{$t("chat.model_load.memory_total")}</span>
          <span
            class="font-medium {refusal ? 'text-destructive' : 'text-foreground'}"
            data-testid="model-load-memory-total"
          >
            {$t("chat.model_load.gigabytes", { values: { size: gb(estimate.memory.totalGb, lang) } })}
          </span>
        {:else if estimate}
          <span class="text-caption text-muted-foreground" data-testid="model-load-memory-unknown">
            {$t("chat.model_load.memory_unknown")}
          </span>
        {/if}
      </div>

      {#if estimateError}
        <p class="text-body-xs text-destructive" role="alert" data-testid="model-load-estimate-error">
          {$t("chat.model_load.estimate_failed", { values: { error: estimateError } })}
        </p>
      {/if}

      <div class="space-y-1.5">
        <p class="text-body-xs font-medium text-foreground">{$t("chat.model_load.file")}</p>
        <div
          class="v4-hair flex flex-wrap items-center gap-2 rounded-md border bg-surface-1 px-3 py-2"
          data-testid="model-load-file"
        >
          <span class="min-w-0 truncate text-body-xs font-medium text-foreground">{ask.modelId}</span>
          {#if format}
            <ExplainedBadge
              label={format.toUpperCase()}
              explanation={$t("chat.target_selector.formats_help")}
              help
              testid="model-load-format-badge"
            />
          {/if}
          {#if estimate?.quantization}
            <ExplainedBadge
              label={estimate.quantization}
              explanation={$t("chat.model_load.quantization_help")}
              testid="model-load-quantization"
            />
          {/if}
        </div>
      </div>

      {#if estimate && !configurable}
        <p class="text-body-xs text-muted-foreground" data-testid="model-load-no-settings">
          {$t("chat.model_load.no_settings")}
        </p>
      {:else if estimate && bounds}
        <div class="space-y-1.5">
          <div class="flex items-center gap-2">
            <label for="model-load-context" class="text-body-xs font-medium text-foreground">
              {$t("chat.model_load.context")}
            </label>
            <Tooltip content={$t("chat.model_load.context_help")} class="max-w-xs">
              <span
                class="inline-flex text-muted-foreground"
                aria-label={$t("chat.model_load.context_help")}
                data-testid="model-load-context-help"
              >
                <CircleHelp size={12} />
              </span>
            </Tooltip>
            <input
              id="model-load-context"
              type="number"
              class="v4-hair ml-auto h-8 w-28 rounded-md border bg-surface-1 px-2 text-right text-body-xs text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
              min={bounds.min}
              max={bounds.max}
              step={bounds.step}
              value={context}
              onchange={setContext}
              disabled={loading}
              data-testid="model-load-context-input"
            />
          </div>
          <p class="text-caption text-muted-foreground" data-testid="model-load-context-max">
            {$t("chat.model_load.context_max", {
              values: { max: bounds.max.toLocaleString(lang) },
            })}
          </p>
          <input
            type="range"
            class="h-1 w-full cursor-pointer accent-primary"
            min={bounds.min}
            max={bounds.max}
            step={bounds.step}
            value={context}
            oninput={setContext}
            disabled={loading}
            aria-label={$t("chat.model_load.context")}
            data-testid="model-load-context-slider"
          />
        </div>

        {#if layers !== null}
          <div class="space-y-1.5">
            <div class="flex items-center gap-2">
              <label for="model-load-offload" class="text-body-xs font-medium text-foreground">
                {$t("chat.model_load.offload")}
              </label>
              <Tooltip content={$t("chat.model_load.offload_help")} class="max-w-xs">
                <span
                  class="inline-flex text-muted-foreground"
                  aria-label={$t("chat.model_load.offload_help")}
                  data-testid="model-load-offload-help"
                >
                  <CircleHelp size={12} />
                </span>
              </Tooltip>
              <span class="ml-auto flex items-center gap-1.5 text-body-xs text-muted-foreground">
                <input
                  id="model-load-offload"
                  type="number"
                  class="v4-hair h-8 w-16 rounded-md border bg-surface-1 px-2 text-right text-body-xs text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
                  min="0"
                  max={layers}
                  step="1"
                  value={offload}
                  onchange={setOffload}
                  disabled={loading}
                  data-testid="model-load-offload-input"
                />
                <span data-testid="model-load-offload-total">/ {layers}</span>
              </span>
            </div>
            <p class="text-caption text-muted-foreground">{$t("chat.model_load.offload_hint")}</p>
            <input
              type="range"
              class="h-1 w-full cursor-pointer accent-primary"
              min="0"
              max={layers}
              step="1"
              value={offload}
              oninput={setOffload}
              disabled={loading}
              aria-label={$t("chat.model_load.offload")}
              data-testid="model-load-offload-slider"
            />
          </div>
        {/if}

        {#if advancedShown}
          <ModelLoadAdvanced bind:form disabled={loading} />
        {/if}
      {/if}

      {#if refusal}
        <div
          class="rounded-lg border border-destructive/40 bg-destructive/5 px-3 py-2.5"
          role="alert"
          data-testid="model-load-refusal"
        >
          <p class="flex items-center gap-2 text-body-xs font-medium text-destructive">
            <TriangleAlert size={14} strokeWidth={1.75} class="shrink-0" />
            {$t("chat.model_load.refusal_title")}
          </p>
          <p class="mt-1 text-body-xs text-foreground" data-testid="model-load-refusal-figures">
            {$t("chat.model_load.refusal_body", {
              values: {
                estimated: gb(refusal.estimatedGb, lang),
                budget: gb(refusal.budgetGb, lang),
                level: $t(`settings.model_defaults.guardrails.${refusal.level}`),
              },
            })}
          </p>
        </div>
      {/if}

      {#if ask.notice && !loadError}
        <p class="text-body-xs text-muted-foreground" data-testid="model-load-notice">{ask.notice}</p>
      {/if}
      {#if loadError}
        <p class="text-body-xs text-destructive" role="alert" data-testid="model-load-error">
          {$t("chat.model_load.load_failed", { values: { error: loadError } })}
        </p>
      {/if}
    </div>

    <div class="mt-5 flex flex-wrap items-end justify-between gap-3">
      <div class="space-y-2">
        {#if configurable}
          <label class="flex items-center gap-2 text-body-xs text-foreground">
            <Checkbox
              bind:checked={form.remember}
              disabled={loading}
              data-testid="model-load-remember"
            />
            <span class="max-w-60 truncate">
              {$t("chat.model_load.remember", { values: { name: ask.modelId } })}
            </span>
          </label>
        {/if}
        <div class="flex items-center gap-2 text-body-xs text-foreground">
          <Toggle
            bind:checked={form.pin}
            size="sm"
            disabled={loading}
            aria-label={$t("chat.model_load.pin")}
            data-testid="model-load-pin"
          />
          <Pin size={12} strokeWidth={1.75} class="text-muted-foreground" />
          <Tooltip content={$t("chat.model_load.pin_help")} class="max-w-xs">
            <span>{$t("chat.model_load.pin")}</span>
          </Tooltip>
        </div>
        {#if configurable}
          <div class="flex items-center gap-2 text-body-xs text-foreground">
            <Toggle
              bind:checked={advancedShown}
              size="sm"
              aria-label={$t("chat.model_load.show_advanced")}
              data-testid="model-load-advanced-toggle"
            />
            <span>{$t("chat.model_load.show_advanced")}</span>
          </div>
        {/if}
      </div>

      <div class="flex items-center gap-2">
        <Button variant="ghost" onclick={close} disabled={loading} data-testid="model-load-cancel">
          {$t("common.cancel")}
        </Button>
        <Button
          variant={refusal ? "outline" : "default"}
          onclick={() => void submit(false)}
          disabled={loading || estimate === null}
          data-testid="model-load-submit"
        >
          {#if loading}
            <Spinner size={13} />
            {$t("chat.model_load.loading")}
          {:else if refusal}
            {$t("chat.model_load.load_anyway")}
          {:else}
            {$t("chat.model_load.load")}
            <kbd class="ml-1 inline-flex items-center gap-0.5 text-overline opacity-80">
              <span>{MOD_LABEL}</span>
              <CornerDownLeft size={11} strokeWidth={2} />
            </kbd>
          {/if}
        </Button>
      </div>
    </div>
  {/if}
</Dialog>

<ConfirmDialog
  open={confirmOpen && refusal !== null}
  onclose={() => (confirmOpen = false)}
  onconfirm={() => void submit(true)}
  title={$t("chat.model_load.confirm_title")}
  message={refusal
    ? $t("chat.model_load.confirm_body", {
        values: {
          name: ask?.modelId ?? "",
          estimated: gb(refusal.estimatedGb, lang),
          budget: gb(refusal.budgetGb, lang),
        },
      })
    : ""}
  confirmLabel={$t("chat.model_load.confirm_action")}
  data-testid="model-load-confirm"
/>
