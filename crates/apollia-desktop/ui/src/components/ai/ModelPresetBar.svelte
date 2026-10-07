<script lang="ts">
  /**
   * ModelPresetBar - the presets of a model, above its Loading and Inference
   * tabs: choose one, save the settings on screen as a new one, save the
   * changes of the chosen one, and make it the default of the model.
   */
  import { t } from "svelte-i18n";
  import type { PresetDetail } from "$lib/ipc/modelDetail";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Select } from "$lib/components/ui/select";
  import ExplainedBadge from "./ExplainedBadge.svelte";
  import { canSetDefault, presetNameError } from "./modelDetail";

  interface Props {
    presets: PresetDetail[];
    /** The preset chosen, `null` for the automatic settings. */
    chosen: PresetDetail | null;
    /** Whether the settings on screen differ from the chosen preset. */
    changed: boolean;
    working: boolean;
    onchoose: (id: number | null) => void;
    onsaveas: (name: string) => void;
    onsave: () => void;
    onsetdefault: () => void;
  }

  let { presets, chosen, changed, working, onchoose, onsaveas, onsave, onsetdefault }: Props =
    $props();

  let naming = $state(false);
  let name = $state("");
  let attempted = $state(false);

  const nameError = $derived(presetNameError(name, presets));

  function choose(event: Event): void {
    const value = (event.currentTarget as HTMLSelectElement).value;
    onchoose(value === "" ? null : Number(value));
  }

  function startNaming(): void {
    naming = true;
    name = "";
    attempted = false;
  }

  function confirm(): void {
    attempted = true;
    if (nameError !== null) return;
    onsaveas(name);
    naming = false;
  }
</script>

<section class="flex flex-col gap-2" aria-label={$t("ai.panel.preset.label")} data-testid="ai-panel-presets">
  <div class="flex items-center gap-2">
    <label for="ai-panel-preset-select" class="text-caption font-medium text-foreground">
      {$t("ai.panel.preset.label")}
    </label>
    {#if chosen?.is_model_default}
      <ExplainedBadge
        label={$t("ai.badge.default")}
        explanation={$t("ai.panel.preset.default_explanation")}
        variant="primary"
        testid="ai-panel-preset-default-badge"
      />
    {/if}
  </div>
  <Select
    size="sm"
    id="ai-panel-preset-select"
    value={chosen ? String(chosen.id) : ""}
    onchange={choose}
    disabled={working}
    data-testid="ai-panel-preset-select"
  >
    <option value="">{$t("ai.panel.preset.none")}</option>
    {#each presets as preset (preset.id)}
      <option value={String(preset.id)}>{preset.name}</option>
    {/each}
  </Select>

  {#if naming}
    <div class="flex flex-col gap-1.5" data-testid="ai-panel-preset-naming">
      <Input
        size="sm"
        bind:value={name}
        placeholder={$t("ai.panel.preset.name_placeholder")}
        aria-label={$t("ai.panel.preset.name_label")}
        aria-invalid={attempted && nameError ? "true" : undefined}
        onkeydown={(event) => {
          if (event.key === "Enter") confirm();
        }}
        data-testid="ai-panel-preset-name"
      />
      {#if attempted && nameError}
        <p class="m-0 text-caption text-destructive" data-testid="ai-panel-preset-name-error">
          {$t(nameError)}
        </p>
      {/if}
      <div class="flex gap-2">
        <Button variant="primary-solid" size="sm" onclick={confirm} disabled={working} data-testid="ai-panel-preset-name-confirm">
          {$t("ai.panel.preset.save_confirm")}
        </Button>
        <Button variant="ghost" size="sm" onclick={() => (naming = false)} data-testid="ai-panel-preset-name-cancel">
          {$t("common.cancel")}
        </Button>
      </div>
    </div>
  {:else}
    <div class="flex flex-wrap gap-2">
      <Button variant="outline" size="sm" onclick={startNaming} disabled={working} data-testid="ai-panel-preset-save-as">
        {$t("ai.panel.preset.save_as")}
      </Button>
      {#if chosen !== null && changed}
        <Button variant="outline" size="sm" onclick={onsave} disabled={working} data-testid="ai-panel-preset-save">
          {$t("ai.panel.preset.save")}
        </Button>
      {/if}
      <Button
        variant="outline"
        size="sm"
        onclick={onsetdefault}
        disabled={working || !canSetDefault(chosen, changed)}
        data-testid="ai-panel-preset-set-default"
      >
        {$t("ai.panel.preset.set_default")}
      </Button>
    </div>
  {/if}
</section>
