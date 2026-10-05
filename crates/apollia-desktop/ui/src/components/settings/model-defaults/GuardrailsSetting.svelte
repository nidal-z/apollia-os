<script lang="ts">
  /**
   * GuardrailsSetting - the load guardrails: five explained levels, the
   * ceiling in gigabytes of the last one, and the mention of the organization
   * policy when it fixes them.
   *
   * One component for the two places that show it, Settings > Model defaults
   * and Settings > Hardware, over the same named settings
   * (`llm.guardrails.level`, `llm.guardrails.ceiling_gb`). It reads and
   * writes them itself, so both places always show the value in force.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { CircleHelp, ShieldCheck } from "lucide-svelte";
  import { Tooltip } from "$lib/components/ui/tooltip";
  import { addToast } from "$lib/components/ui/toast";
  import { getHardwareProfile } from "$lib/ipc/modelHub";
  import {
    GUARDRAIL_CHOICES,
    blockLock,
    fieldFaultKey,
    initialModelDefaults,
    loadModelDefaults,
    readField,
    writeModelDefault,
    type ModelDefaultsIpc,
    type ModelDefaultsState,
    type ModelDefaultsValues,
  } from "$lib/settings/modelDefaults";
  import SettingsSection from "../SettingsSection.svelte";
  import SettingOption from "./SettingOption.svelte";
  import PolicyLockedNote from "./PolicyLockedNote.svelte";

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
  /** Why the ceiling typed last was refused, already translated. */
  let ceilingError = $state<string | null>(null);
  /** The choice whose write went through last. */
  let savedAt = $state<string | null>(null);
  /** "Ceiling" chosen while no ceiling is written yet: the level waits for it. */
  let ceilingPending = $state(false);
  let savedTimer: ReturnType<typeof setTimeout> | undefined;

  const lock = $derived(blockLock("guardrails", defaults.sources));
  const values = $derived(defaults.values);
  const shown = $derived(ceilingPending ? "ceiling" : values.guardrailLevel);

  onMount(() => {
    void loadModelDefaults(ipc).then((loaded) => (defaults = loaded));
    void getHardwareProfile()
      .then((profile) => (machineMemoryGb = profile.total_capacity_gb))
      .catch(() => (machineMemoryGb = null));
    return () => clearTimeout(savedTimer);
  });

  async function write<F extends "guardrailLevel" | "guardrailCeiling">(
    field: F,
    value: ModelDefaultsValues[F],
    choice: string,
  ): Promise<boolean> {
    const outcome = await writeModelDefault(defaults, field, value, ipc);
    defaults = outcome.state;
    if (outcome.written) {
      savedAt = choice;
      clearTimeout(savedTimer);
      savedTimer = setTimeout(() => (savedAt = null), SAVED_MS);
    } else {
      addToast($t("settings.model_defaults.write_failed"), "error");
    }
    return outcome.written;
  }

  function choose(level: string) {
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
    const reading = readField("guardrailCeiling", typed, machineMemoryGb);
    if (!reading.ok) {
      ceilingError = $t(fieldFaultKey(reading.fault), {
        values: { min: reading.min, max: reading.max },
      });
      return;
    }
    ceilingError = null;
    if (!(await write("guardrailCeiling", reading.value, "ceiling"))) return;
    if (values.guardrailLevel !== "ceiling") {
      ceilingPending = false;
      await write("guardrailLevel", "ceiling", "ceiling");
    }
  }
</script>

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
    <PolicyLockedNote labelKey={lock.policyLabelKey} data-testid="model-defaults-guardrails-policy" />
  {/if}
  <div
    role="radiogroup"
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
        checked={shown === choice.value}
        disabled={lock.disabled}
        onchoose={choose}
        figure={choice.value === "ceiling"
          ? values.guardrailCeiling > 0
            ? String(values.guardrailCeiling)
            : ""
          : undefined}
        unit={$t("settings.model_defaults.guardrails.ceiling_unit")}
        oncommit={(typed) => void commitCeiling(typed)}
        error={choice.value === "ceiling" ? ceilingError : null}
        saved={savedAt === choice.value}
        data-testid={choice.testid}
      />
    {/each}
  </div>
</SettingsSection>
