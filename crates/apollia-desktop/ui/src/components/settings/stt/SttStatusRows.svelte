<script lang="ts">
  /**
   * SttStatusRows - the read-only engine diagnostics stacked inside the STT
   * status section (engine, loaded model, backend, microphone). Each value is a
   * label + a token-coloured status dot, matching the settings field-row rhythm.
   */
  import { t } from "svelte-i18n";
  import SettingsFieldRow from "../SettingsFieldRow.svelte";
  import { sttStatus } from "$lib/stores/stt";
  import { modelRowState } from "$lib/stt/modelRowState";

  interface Props {
    /** True when no audio input device is present, independent of engine status. */
    noMicrophone?: boolean;
  }

  let { noMicrophone = false }: Props = $props();

  const micOk = $derived($sttStatus?.input_available !== false && !noMicrophone);
  const modelRow = $derived(modelRowState($sttStatus));
</script>

<SettingsFieldRow label={$t("settings.stt_engine_status")}>
  {#snippet control()}
    <span class="inline-flex items-center gap-1.5 text-body-sm text-foreground">
      <span
        class="h-2 w-2 rounded-full {$sttStatus?.enabled ? 'bg-success' : 'bg-muted-foreground'}"
      ></span>
      {$sttStatus?.enabled ? $t("settings.stt_enabled") : $t("settings.stt_disabled")}
    </span>
  {/snippet}
</SettingsFieldRow>

<SettingsFieldRow label={$t("settings.stt_model_name")}>
  {#snippet control()}
    {#if modelRow === "loaded"}
      <span class="inline-flex items-center gap-1.5 text-body-sm text-foreground" data-testid="stt-model-loaded">
        <span class="h-2 w-2 rounded-full bg-success"></span>
        {$sttStatus?.model_name}
      </span>
    {:else if modelRow === "ready"}
      <span class="inline-flex items-center gap-1.5 text-body-sm text-foreground" data-testid="stt-model-ready">
        <span class="h-2 w-2 rounded-full bg-warning"></span>
        {$sttStatus?.model_name}
        <span class="text-muted-foreground">{$t("settings.stt_model_ready_hint")}</span>
      </span>
    {:else}
      <span class="text-body-sm text-muted-foreground">{$t("settings.stt_model_not_loaded")}</span>
    {/if}
  {/snippet}
</SettingsFieldRow>

<SettingsFieldRow label={$t("settings.stt_backend")}>
  {#snippet control()}
    <span class="font-mono text-body-sm text-foreground">{$sttStatus?.backend_name ?? "-"}</span>
  {/snippet}
</SettingsFieldRow>

<SettingsFieldRow label={$t("settings.stt_microphone")} border={false}>
  {#snippet control()}
    <span
      class="inline-flex items-center gap-1.5 text-body-sm text-foreground"
      data-testid={micOk ? undefined : "stt-status-no-mic"}
    >
      <span class="h-2 w-2 rounded-full {micOk ? 'bg-success' : 'bg-warning'}"></span>
      {micOk ? $t("settings.stt_microphone_ok") : $t("settings.stt_microphone_none")}
    </span>
  {/snippet}
</SettingsFieldRow>
