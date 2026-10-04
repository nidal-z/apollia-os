<script lang="ts">
  /**
   * Settings > Model defaults: the image ceiling, the default context window,
   * the load guardrails, the bypass of a memory warning and the unload delay.
   *
   * Every value is written the moment it is chosen. A block whose value the
   * organization policy fixes keeps that value shown, with its choices
   * disabled and the policy named.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { CircleHelp, Gauge, Image, ShieldAlert, ShieldCheck, Timer } from "lucide-svelte";
  import { Toggle } from "$lib/components/ui/toggle";
  import { Input } from "$lib/components/ui/input";
  import { Tooltip } from "$lib/components/ui/tooltip";
  import { addToast } from "$lib/components/ui/toast";
  import { getHardwareProfile } from "$lib/ipc/modelHub";
  import {
    BYPASS_CHOICES,
    CONTEXT_CHOICES,
    DEFAULT_IDLE_UNLOAD_MINUTES,
    GUARDRAIL_CHOICES,
    MODEL_DEFAULT_BLOCKS,
    UNLOAD_CHOICES,
    blockLock,
    fieldFaultKey,
    initialModelDefaults,
    loadModelDefaults,
    readField,
    unloadMode,
    writeModelDefault,
    type BoundedField,
    type ModelDefaultBlock,
    type ModelDefaultField,
    type ModelDefaultsIpc,
    type ModelDefaultsState,
    type ModelDefaultsValues,
  } from "$lib/settings/modelDefaults";
  import SettingsSubPage from "../../components/settings/SettingsSubPage.svelte";
  import SettingsSection from "../../components/settings/SettingsSection.svelte";
  import SettingOption from "../../components/settings/model-defaults/SettingOption.svelte";
  import PolicyLockedNote from "../../components/settings/model-defaults/PolicyLockedNote.svelte";

  interface Props {
    /** The state shown before the shell answers; the defaults when omitted. */
    initial?: ModelDefaultsState;
    /** The shell calls, replaceable in tests. */
    ipc?: ModelDefaultsIpc;
  }

  let { initial, ipc }: Props = $props();

  /** How long "saved" stays under a choice, in milliseconds. */
  const SAVED_MS = 2000;

  // The shell's answer replaces this seed once mounted.
  // svelte-ignore state_referenced_locally
  let defaults = $state<ModelDefaultsState>(initial ?? initialModelDefaults());
  /** What this machine can hold, in gigabytes; `null` until the probe answers. */
  let machineMemoryGb = $state<number | null>(null);
  /** The refused figure of each field, already translated. */
  let errors = $state<Partial<Record<BoundedField, string>>>({});
  /** The choice whose write went through last. */
  let savedAt = $state<string | null>(null);
  /** "Ceiling" chosen while no ceiling is written yet: the level waits for it. */
  let ceilingPending = $state(false);
  let savedTimer: ReturnType<typeof setTimeout> | undefined;

  const locks = $derived(
    Object.fromEntries(
      MODEL_DEFAULT_BLOCKS.map((block) => [block, blockLock(block, defaults.sources)]),
    ) as Record<ModelDefaultBlock, ReturnType<typeof blockLock>>,
  );
  const values = $derived(defaults.values);
  const guardrailShown = $derived(ceilingPending ? "ceiling" : values.guardrailLevel);

  onMount(() => {
    void loadModelDefaults(ipc).then((loaded) => (defaults = loaded));
    void getHardwareProfile()
      .then((profile) => (machineMemoryGb = profile.total_capacity_gb))
      .catch(() => (machineMemoryGb = null));
    return () => clearTimeout(savedTimer);
  });

  function markSaved(choice: string) {
    savedAt = choice;
    clearTimeout(savedTimer);
    savedTimer = setTimeout(() => (savedAt = null), SAVED_MS);
  }

  async function write<F extends ModelDefaultField>(
    field: F,
    value: ModelDefaultsValues[F],
    choice: string,
  ): Promise<boolean> {
    const outcome = await writeModelDefault(defaults, field, value, ipc);
    defaults = outcome.state;
    if (outcome.written) markSaved(choice);
    else addToast($t("settings.model_defaults.write_failed"), "error");
    return outcome.written;
  }

  /** Reads a typed figure; a refused one shows its reason and answers `null`. */
  function figureOf(field: BoundedField, typed: string): number | null {
    const reading = readField(field, typed, machineMemoryGb);
    if (reading.ok) {
      errors = { ...errors, [field]: undefined };
      return reading.value;
    }
    errors = {
      ...errors,
      [field]: $t(fieldFaultKey(reading.fault), {
        values: { min: reading.min, max: reading.max },
      }),
    };
    return null;
  }

  function commitImageSide(event: Event) {
    const side = figureOf("imageMaxSide", (event.currentTarget as HTMLInputElement).value);
    if (side !== null) void write("imageMaxSide", side, "images");
  }

  function commitContext(typed: string) {
    // An emptied field hands the window back to the engine.
    if (typed.trim() === "") {
      errors = { ...errors, contextTokens: undefined };
      void write("contextTokens", 0, "custom");
      return;
    }
    const tokens = figureOf("contextTokens", typed);
    if (tokens !== null) void write("contextTokens", tokens, "custom");
  }

  function chooseGuardrail(level: string) {
    const choice = GUARDRAIL_CHOICES.find((c) => c.value === level);
    if (!choice) return;
    // A ceiling level without a ceiling holds to "balanced" in the engine, so
    // the level is only written once a ceiling is.
    if (choice.value === "ceiling" && values.guardrailCeiling <= 0) {
      ceilingPending = true;
      return;
    }
    ceilingPending = false;
    void write("guardrailLevel", choice.value, choice.value);
  }

  async function commitCeiling(typed: string) {
    const ceiling = figureOf("guardrailCeiling", typed);
    if (ceiling === null) return;
    if (!(await write("guardrailCeiling", ceiling, "ceiling"))) return;
    if (values.guardrailLevel !== "ceiling") {
      ceilingPending = false;
      await write("guardrailLevel", "ceiling", "ceiling");
    }
  }

  function chooseUnload(mode: string) {
    if (mode === "never") void write("idleUnload", 0, "never");
    else if (values.idleUnload <= 0) void write("idleUnload", DEFAULT_IDLE_UNLOAD_MINUTES, "after");
  }

  function commitUnload(typed: string) {
    const minutes = figureOf("idleUnload", typed);
    if (minutes !== null) void write("idleUnload", minutes, "after");
  }
</script>

<SettingsSubPage route="model-defaults" data-testid="model-defaults-section">
  {#each MODEL_DEFAULT_BLOCKS as block (block)}
    {@const lock = locks[block]}
    {#if block === "images"}
      <SettingsSection
        title={$t("settings.model_defaults.images.title")}
        data-testid="model-defaults-images"
      >
        {#snippet icon()}<Image size={15} strokeWidth={1.75} />{/snippet}
        <div class="flex items-start justify-between gap-4">
          <div class="min-w-0 flex-1">
            <div class="flex flex-wrap items-center gap-2">
              <label for="model-defaults-image-side" class="text-label-md text-foreground">
                {$t("settings.model_defaults.images.never_exceed")}
              </label>
              <Input
                id="model-defaults-image-side"
                size="sm"
                inputmode="numeric"
                class={errors.imageMaxSide ? "w-24 border-destructive text-right" : "w-24 text-right"}
                value={String(values.imageMaxSide)}
                disabled={lock.disabled || !values.imageLimit}
                aria-invalid={errors.imageMaxSide ? "true" : undefined}
                onchange={commitImageSide}
                data-testid="model-defaults-image-side"
              />
              <span class="text-caption text-muted-foreground">
                {$t("settings.model_defaults.images.unit")}
              </span>
            </div>
            <p class="mt-1 max-w-md text-caption text-muted-foreground">
              {$t("settings.model_defaults.images.hint")}
            </p>
            {#if errors.imageMaxSide}
              <p
                class="mt-1 text-caption text-destructive"
                role="alert"
                data-testid="model-defaults-image-side-error"
              >
                {errors.imageMaxSide}
              </p>
            {:else if savedAt === "images"}
              <p class="mt-1 text-caption text-success" role="status">
                {$t("settings.model_defaults.saved")}
              </p>
            {/if}
            {#if lock.policyLabelKey}
              <PolicyLockedNote
                labelKey={lock.policyLabelKey}
                data-testid="model-defaults-images-policy"
              />
            {/if}
          </div>
          <Toggle
            checked={values.imageLimit}
            disabled={lock.disabled}
            onchange={(on) => void write("imageLimit", on, "images")}
            aria-label={$t("settings.model_defaults.images.toggle_label")}
            data-testid="model-defaults-image-limit"
          />
        </div>
      </SettingsSection>
    {:else if block === "context"}
      <SettingsSection
        title={$t("settings.model_defaults.context.title")}
        description={$t("settings.model_defaults.context.description")}
        bodyLayout="flush"
        data-testid="model-defaults-context"
      >
        {#snippet icon()}<Gauge size={15} strokeWidth={1.75} />{/snippet}
        {#if lock.policyLabelKey}
          <PolicyLockedNote
            labelKey={lock.policyLabelKey}
            data-testid="model-defaults-context-policy"
          />
        {/if}
        <div role="radiogroup" aria-label={$t("settings.model_defaults.context.title")} class="flex flex-col">
          {#each CONTEXT_CHOICES as choice (choice.value)}
            <SettingOption
              value={choice.value}
              label={$t(choice.labelKey)}
              hint={$t(choice.hintKey)}
              checked={values.contextMode === choice.value}
              disabled={lock.disabled}
              onchoose={() => void write("contextMode", choice.value, choice.value)}
              figure={choice.value === "custom"
                ? values.contextTokens > 0
                  ? String(values.contextTokens)
                  : ""
                : undefined}
              placeholder={$t("settings.model_defaults.context.auto_placeholder")}
              unit={$t("settings.model_defaults.context.unit")}
              oncommit={commitContext}
              error={choice.value === "custom" ? errors.contextTokens : null}
              saved={savedAt === choice.value}
              data-testid={choice.testid}
            />
          {/each}
        </div>
      </SettingsSection>
    {:else if block === "guardrails"}
      <SettingsSection
        title={$t("settings.model_defaults.guardrails.title")}
        bodyLayout="flush"
        data-testid="model-defaults-guardrails"
      >
        {#snippet icon()}<ShieldCheck size={15} strokeWidth={1.75} />{/snippet}
        <div class="flex items-center gap-1.5">
          <p class="text-label-md text-foreground">
            {$t("settings.model_defaults.guardrails.level_label")}
          </p>
          <Tooltip content={$t("settings.model_defaults.guardrails.help")} side="bottom" class="max-w-xs">
            <button
              type="button"
              class="inline-flex h-5 w-5 items-center justify-center rounded-full text-muted-foreground ring-offset-background transition-colors duration-fast hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
              aria-label={$t("settings.model_defaults.guardrails.help_label")}
              data-testid="model-defaults-guardrails-help"
            >
              <CircleHelp class="h-3.5 w-3.5" aria-hidden="true" />
            </button>
          </Tooltip>
        </div>
        {#if lock.policyLabelKey}
          <PolicyLockedNote
            labelKey={lock.policyLabelKey}
            data-testid="model-defaults-guardrails-policy"
          />
        {/if}
        <div role="radiogroup"
          aria-label={$t("settings.model_defaults.guardrails.title")}
          aria-disabled={lock.disabled}
          class="flex flex-col"
          data-testid="model-defaults-guardrail-selector"
        >
          {#each GUARDRAIL_CHOICES as choice (choice.value)}
            <SettingOption
              value={choice.value}
              label={$t(choice.labelKey)}
              hint={$t(choice.hintKey)}
              checked={guardrailShown === choice.value}
              disabled={lock.disabled}
              onchoose={chooseGuardrail}
              figure={choice.value === "ceiling"
                ? values.guardrailCeiling > 0
                  ? String(values.guardrailCeiling)
                  : ""
                : undefined}
              unit={$t("settings.model_defaults.guardrails.ceiling_unit")}
              oncommit={(typed) => void commitCeiling(typed)}
              error={choice.value === "ceiling" ? errors.guardrailCeiling : null}
              saved={savedAt === choice.value}
              data-testid={choice.testid}
            />
          {/each}
        </div>
      </SettingsSection>
    {:else if block === "bypass"}
      <SettingsSection
        title={$t("settings.model_defaults.bypass.title")}
        bodyLayout="flush"
        data-testid="model-defaults-bypass"
      >
        {#snippet icon()}<ShieldAlert size={15} strokeWidth={1.75} />{/snippet}
        <p class="text-label-md text-foreground">
          {$t("settings.model_defaults.bypass.load_anyway")}
        </p>
        <p class="mt-0.5 max-w-md text-caption text-muted-foreground">
          {$t("settings.model_defaults.bypass.load_anyway_hint")}
        </p>
        {#if lock.policyLabelKey}
          <PolicyLockedNote
            labelKey={lock.policyLabelKey}
            data-testid="model-defaults-bypass-policy"
          />
        {/if}
        <div role="radiogroup" aria-label={$t("settings.model_defaults.bypass.title")} class="flex flex-col">
          {#each BYPASS_CHOICES as choice (choice.value)}
            <SettingOption
              value={choice.value}
              label={$t(choice.labelKey)}
              hint={$t(choice.hintKey)}
              checked={values.bypass === choice.value}
              disabled={lock.disabled}
              onchoose={() => void write("bypass", choice.value, choice.value)}
              saved={savedAt === choice.value}
              data-testid={choice.testid}
            />
          {/each}
        </div>
      </SettingsSection>
    {:else}
      <SettingsSection
        title={$t("settings.model_defaults.unload.title")}
        bodyLayout="flush"
        data-testid="model-defaults-unload"
      >
        {#snippet icon()}<Timer size={15} strokeWidth={1.75} />{/snippet}
        <p class="text-label-md text-foreground">
          {$t("settings.model_defaults.unload.label")}
        </p>
        <p class="mt-0.5 max-w-md text-caption text-muted-foreground">
          {$t("settings.model_defaults.unload.label_hint")}
        </p>
        {#if lock.policyLabelKey}
          <PolicyLockedNote
            labelKey={lock.policyLabelKey}
            data-testid="model-defaults-unload-policy"
          />
        {/if}
        <div role="radiogroup" aria-label={$t("settings.model_defaults.unload.label")} class="flex flex-col">
          {#each UNLOAD_CHOICES as choice (choice.value)}
            <SettingOption
              value={choice.value}
              label={$t(choice.labelKey)}
              hint={$t(choice.hintKey)}
              checked={unloadMode(values.idleUnload) === choice.value}
              disabled={lock.disabled}
              onchoose={chooseUnload}
              figure={choice.value === "after"
                ? String(values.idleUnload > 0 ? values.idleUnload : DEFAULT_IDLE_UNLOAD_MINUTES)
                : undefined}
              unit={$t("settings.model_defaults.unload.unit")}
              oncommit={commitUnload}
              error={choice.value === "after" ? errors.idleUnload : null}
              saved={savedAt === choice.value}
              data-testid={choice.testid}
            />
          {/each}
        </div>
      </SettingsSection>
    {/if}
  {/each}
</SettingsSubPage>
