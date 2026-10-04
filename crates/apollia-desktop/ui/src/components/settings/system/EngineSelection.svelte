<script lang="ts" module>
  import type { EngineFormat } from "$lib/settings/engineSelection";

  /** The name of each format, a proper noun, never translated. */
  export const ENGINE_FORMAT_NAMES: Record<EngineFormat, string> = {
    gguf: "GGUF",
    mlx: "MLX",
  };

  /**
   * The formats this machine can serve: GGUF everywhere, MLX on a Mac with
   * Apple Silicon only. An unknown machine shows GGUF alone.
   */
  export function servedFormats(acceleratorKind: string | null): EngineFormat[] {
    return acceleratorKind === "apple_silicon" ? ["gguf", "mlx"] : ["gguf"];
  }
</script>

<script lang="ts">
  /**
   * EngineSelection - System > Engines, "Engine per format": one line per
   * model format this machine serves, each a list that opens on the
   * automatic choice and then names every engine the machine holds for that
   * format with the version it states; then the automatic update of the
   * chosen engines. A chosen variant the machine no longer holds reads as the
   * automatic choice, with a warning. The values are settings of
   * `apollia.toml`; a value the organization fixes locks its control.
   */
  import { t } from "svelte-i18n";
  import { Lock, SlidersHorizontal } from "lucide-svelte";
  import { Select } from "$lib/components/ui/select";
  import { Toggle } from "$lib/components/ui/toggle";
  import { Banner } from "$lib/components/ui/banner";
  import SettingsSection from "../SettingsSection.svelte";
  import SettingsFieldRow from "../SettingsFieldRow.svelte";
  import { variantLabel } from "./EngineList.svelte";
  import type { EngineEntry, EnginesResponse } from "$lib/ipc/system";
  import {
    engineInService,
    engineOptions,
    engineSelection,
    isEngineChoice,
    type EngineChoice,
    type EngineOption,
    type EngineSettingsState,
  } from "$lib/settings/engineSelection";

  interface Props {
    response: EnginesResponse | null;
    /** `accelerator.kind` of the hardware probe; `null` until it answers. */
    acceleratorKind: string | null;
    settings: EngineSettingsState;
    onchoose: (format: EngineFormat, choice: EngineChoice) => void;
    onautoupdate: (on: boolean) => void;
  }

  let { response, acceleratorKind, settings, onchoose, onautoupdate }: Props = $props();

  const ENGINE_NAMES: Record<EngineEntry["engine"], string> = {
    "llama-server": "llama.cpp",
    "vllm-mlx": "vllm-mlx",
    nodejs: "Node.js",
  };

  const HINT_KEYS: Record<EngineFormat, string> = {
    gguf: "settings.engines.format_gguf_hint",
    mlx: "settings.engines.format_mlx_hint",
  };

  function buildName(engine: EngineEntry["engine"], variant: EngineEntry["variant"]): string {
    const label = variantLabel(variant, $t("settings.engines.variant_cpu"));
    return label ? `${ENGINE_NAMES[engine]} · ${label}` : ENGINE_NAMES[engine];
  }

  function optionLabel(option: EngineOption): string {
    const name = buildName(option.engine, option.variant);
    const version = option.version ?? $t("settings.engines.version_unknown");
    return option.embedded
      ? $t("settings.engines.option_embedded", { values: { name, version } })
      : `${name} · ${version}`;
  }

  const lines = $derived(
    servedFormats(acceleratorKind).map((format) => {
      const options = engineOptions(response, format);
      const selection = engineSelection(settings[format].value, options);
      const serving = engineInService(response, format);
      return {
        format,
        options,
        selection,
        locked: settings[format].source === "policy",
        serving: serving ? buildName(serving.engine, serving.variant) : null,
      };
    }),
  );

  const fallen = $derived(lines.filter((line) => line.selection.fellBack));
  const autoUpdateLocked = $derived(settings.autoUpdate.source === "policy");
</script>

<SettingsSection
  title={$t("settings.engines.selection_title")}
  description={$t("settings.engines.selection_desc")}
  bodyLayout="flush"
  data-testid="engines-selection"
>
  {#snippet icon()}<SlidersHorizontal size={15} strokeWidth={1.75} />{/snippet}

  {#each fallen as line (line.format)}
    <Banner
      variant="warning"
      surface="card"
      class="mb-2"
      data-testid="engine-format-{line.format}-fell-back"
    >
      {$t("settings.engines.selection_fell_back", {
        values: { format: ENGINE_FORMAT_NAMES[line.format] },
      })}
    </Banner>
  {/each}

  {#each lines as line (line.format)}
    <SettingsFieldRow
      label={ENGINE_FORMAT_NAMES[line.format]}
      hint={line.serving
        ? `${$t(HINT_KEYS[line.format])} ${$t("settings.engines.format_serving", { values: { name: line.serving } })}`
        : $t(HINT_KEYS[line.format])}
      for="engine-format-{line.format}-select"
      data-testid="engine-format-{line.format}"
    >
      {#snippet control()}
        <div class="flex flex-col items-end gap-1">
          <Select
            id="engine-format-{line.format}-select"
            size="sm"
            value={line.selection.value}
            disabled={line.locked || !response}
            onchange={(e) => {
              const chosen = e.currentTarget.value;
              if (isEngineChoice(chosen)) onchoose(line.format, chosen);
            }}
            aria-label={$t("settings.engines.format_label", {
              values: { format: ENGINE_FORMAT_NAMES[line.format] },
            })}
            data-testid="engine-format-{line.format}-select"
          >
            <option value="auto">{$t("settings.engines.option_auto")}</option>
            {#each line.options as option (option.value)}
              <option value={option.value}>{optionLabel(option)}</option>
            {/each}
          </Select>
          {#if line.locked}
            <p
              class="inline-flex items-center gap-1.5 text-xs text-muted-foreground"
              title={$t("settings.policy_locked_hint")}
              data-testid="engine-format-{line.format}-policy"
            >
              <Lock class="h-3 w-3" aria-hidden="true" />
              {$t("settings.policy_locked")}
            </p>
          {/if}
        </div>
      {/snippet}
    </SettingsFieldRow>
  {/each}

  <SettingsFieldRow
    label={$t("settings.engines.auto_update_label")}
    hint={$t("settings.engines.auto_update_hint")}
    for="engines-auto-update-toggle"
    border={false}
    data-testid="engines-auto-update"
  >
    {#snippet control()}
      <div class="flex flex-col items-end gap-1">
        <Toggle
          id="engines-auto-update-toggle"
          checked={settings.autoUpdate.value}
          disabled={autoUpdateLocked}
          onchange={onautoupdate}
          aria-label={$t("settings.engines.auto_update_label")}
          data-testid="engines-auto-update-toggle"
        />
        {#if autoUpdateLocked}
          <p
            class="inline-flex items-center gap-1.5 text-xs text-muted-foreground"
            title={$t("settings.policy_locked_hint")}
            data-testid="engines-auto-update-policy"
          >
            <Lock class="h-3 w-3" aria-hidden="true" />
            {$t("settings.policy_locked")}
          </p>
        {/if}
      </div>
    {/snippet}
  </SettingsFieldRow>
</SettingsSection>
