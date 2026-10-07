<script module lang="ts">
  import type { PresetOutcome } from "./useSessionSettings.svelte";

  /** The i18n key of the reason a preset was not created; `null` when the runtime's own text is shown. */
  export function presetErrorKey(outcome: PresetOutcome): string | null {
    if (outcome.ok) return null;
    switch (outcome.reason) {
      case "not_json":
        return "chat.session_settings.preset.error_not_json";
      case "not_a_preset":
        return "chat.session_settings.preset.error_not_a_preset";
      case "no_model":
        return "chat.session_settings.preset.error_no_model";
      case "refused":
        return null;
    }
  }
</script>

<script lang="ts">
  /**
   * The preset of a conversation: the one it chose, what it changed over it,
   * and the ways to get another (a new one, a file).
   */
  import { t } from "svelte-i18n";
  import { FilePlus2, Upload, Undo2 } from "lucide-svelte";
  import { Select } from "$lib/components/ui/select";
  import { Button } from "$lib/components/ui/button";
  import { ActionMenu } from "$lib/components/ui/action-menu";
  import { PRESET_FIELD_KEYS } from "$lib/chat/sessionSettings";
  import type { SessionSettingsController } from "./useSessionSettings.svelte";
  import PresetNameDialog from "./PresetNameDialog.svelte";

  interface Props {
    controller: SessionSettingsController;
    disabled: boolean;
  }

  let { controller, disabled }: Props = $props();

  let naming = $state(false);
  let fileInput = $state<HTMLInputElement | null>(null);
  let failure = $state<string | null>(null);

  const noModel = $derived(controller.model === null);
  const modifiedNames = $derived(
    controller.modified.map((field) => $t(PRESET_FIELD_KEYS[field])).join(", "),
  );

  function report(outcome: PresetOutcome): void {
    if (outcome.ok) {
      failure = null;
      return;
    }
    const key = presetErrorKey(outcome);
    failure = key
      ? $t(key)
      : $t("chat.session_settings.preset.error_refused", {
          values: { reason: outcome.detail ?? "" },
        });
  }

  async function handleFile(event: Event): Promise<void> {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file) return;
    let text: string;
    try {
      text = await file.text();
    } catch {
      report({ ok: false, reason: "not_json" });
      return;
    }
    report(await controller.importPreset(text));
  }

  async function createNamed(name: string): Promise<string | null> {
    const outcome = await controller.newPreset(name);
    if (outcome.ok) {
      naming = false;
      failure = null;
      return null;
    }
    const key = presetErrorKey(outcome);
    return key
      ? $t(key)
      : $t("chat.session_settings.preset.error_refused", {
          values: { reason: outcome.detail ?? "" },
        });
  }
</script>

<section class="space-y-2">
  <div class="flex items-center justify-between gap-2">
    <label for="session-preset-select" class="text-xs font-medium text-foreground">
      {$t("chat.session_settings.preset.title")}
    </label>
    {#if controller.modified.length > 0}
      <span
        class="inline-flex items-center gap-1.5 text-micro text-text-warning"
        data-testid="session-preset-modified"
      >
        <span class="inline-block h-1.5 w-1.5 rounded-full bg-warning" aria-hidden="true"></span>
        {$t("chat.session_settings.preset.modified")}
      </span>
    {/if}
  </div>

  <div class="flex items-center gap-1.5">
    <Select
      id="session-preset-select"
      size="sm"
      class="min-w-0 flex-1"
      value={controller.preset ? String(controller.preset.id) : ""}
      onchange={(e: Event) => {
        const raw = (e.currentTarget as HTMLSelectElement).value;
        failure = null;
        void controller.choose(raw === "" ? null : Number(raw));
      }}
      disabled={disabled || noModel}
      data-testid="session-preset-select"
    >
      <option value="" data-testid="session-preset-option-none">
        {$t("chat.session_settings.preset.none")}
      </option>
      {#each controller.presets as preset (preset.id)}
        <option value={String(preset.id)} data-testid="session-preset-option-{preset.id}">
          {preset.name}
        </option>
      {/each}
    </Select>
    <ActionMenu
      triggerLabel={$t("chat.session_settings.preset.menu")}
      data-testid="session-preset-menu"
      items={[
        {
          id: "new",
          label: $t("chat.session_settings.preset.new"),
          icon: FilePlus2,
          onclick: () => (naming = true),
          disabled: disabled || noModel,
          testid: "session-preset-new",
        },
        {
          id: "import",
          label: $t("chat.session_settings.preset.import"),
          icon: Upload,
          onclick: () => fileInput?.click(),
          disabled: disabled || noModel,
          testid: "session-preset-import",
        },
      ]}
    />
    <input
      bind:this={fileInput}
      type="file"
      accept=".json,application/json"
      class="hidden"
      onchange={handleFile}
      data-testid="session-preset-file"
    />
  </div>

  {#if controller.modified.length > 0}
    <div class="flex items-center justify-between gap-2">
      <p class="min-w-0 text-micro text-muted-foreground" data-testid="session-preset-differs">
        {$t("chat.session_settings.preset.differs", { values: { fields: modifiedNames } })}
      </p>
      <Button
        variant="ghost"
        size="sm"
        class="shrink-0 text-xs"
        onclick={() => void controller.discard()}
        {disabled}
        data-testid="session-preset-discard"
      >
        {#snippet icon()}<Undo2 size={12} />{/snippet}
        {$t("chat.session_settings.preset.discard")}
      </Button>
    </div>
  {/if}
  {#if noModel}
    <p class="text-micro text-muted-foreground" data-testid="session-preset-no-model">
      {$t("chat.session_settings.preset.error_no_model")}
    </p>
  {:else if controller.modified.length === 0}
    <p class="text-micro text-muted-foreground">{$t("chat.session_settings.preset.hint")}</p>
  {/if}

  {#if failure}
    <p class="text-micro text-destructive" role="alert" data-testid="session-preset-error">
      {failure}
    </p>
  {/if}
</section>

<PresetNameDialog
  open={naming}
  title={$t("chat.session_settings.preset.new")}
  onclose={() => (naming = false)}
  onsubmit={createNamed}
  testid="session-preset-new-dialog"
/>
