<script lang="ts">
  /**
   * AboutBuildSection - the build and engine facts of this installation, each
   * copyable, plus a copy-ready diagnostic block composed only from real,
   * present fields. Shown under System > Engines.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { Info, Copy, Check } from "lucide-svelte";
  import { systemInfoStore, settingsLoaders } from "$lib/stores/settings";
  import { Button } from "$lib/components/ui/button";
  import { addToast } from "$lib/components/ui/toast";
  import { reportError } from "$lib/errors/reportError";
  import SettingsSection from "../SettingsSection.svelte";
  import SettingsFieldRow from "../SettingsFieldRow.svelte";
  import CopyValue from "./CopyValue.svelte";

  const LICENSE_SPDX = "MIT OR Apache-2.0";
  const ENGINE = "llama-server (llama.cpp)";
  const STT_ENGINE = "whisper-rs (GGML)";

  const version = $derived($systemInfoStore.data?.version ?? null);
  const os = $derived($systemInfoStore.data?.os ?? null);
  const pythonPath = $derived($systemInfoStore.data?.python_path ?? null);
  const dataDir = $derived($systemInfoStore.data?.data_dir ?? null);
  const unavailable = $derived(
    $systemInfoStore.data === null && $systemInfoStore.error !== null,
  );

  // A copy-ready diagnostic block composed only from real, present fields.
  const diagnostic = $derived.by(() => {
    const lines = [
      `Apollia OS v${version ?? $t("settings.about.diag_unknown")}`,
      `${$t("settings.about.diag_platform")}: ${os ?? $t("settings.about.diag_unknown")}`,
    ];
    if (pythonPath) lines.push(`Python: ${pythonPath}`);
    if (dataDir) lines.push(`${$t("settings.about.diag_data_dir")}: ${dataDir}`);
    lines.push(
      `${$t("settings.about.diag_engine")}: ${ENGINE}`,
      `${$t("settings.about.diag_transcription")}: ${STT_ENGINE}`,
      `${$t("settings.about.diag_license")}: ${LICENSE_SPDX}`,
    );
    return lines.join("\n");
  });

  let diagCopied = $state(false);
  let diagTimer: ReturnType<typeof setTimeout> | undefined;

  onMount(() => {
    void settingsLoaders.systemInfo();
  });

  async function copyDiagnostic(): Promise<void> {
    try {
      await navigator.clipboard.writeText(diagnostic);
      diagCopied = true;
      addToast($t("settings.about.copy_success"), "success");
      clearTimeout(diagTimer);
      diagTimer = setTimeout(() => (diagCopied = false), 1500);
    } catch (err) {
      reportError(err, { surface: "toast" });
    }
  }
</script>

<SettingsSection
  title={$t("settings.about.build_title")}
  description={$t("settings.about.build_desc")}
  bodyLayout="flush"
  data-testid="about-build"
>
  {#snippet icon()}<Info size={15} strokeWidth={1.75} />{/snippet}

  {#if unavailable}
    <p class="py-3 text-body-sm text-muted-foreground" data-testid="about-info-unavailable">
      {$t("settings.about.info_unavailable")}
    </p>
  {:else}
    <SettingsFieldRow label={$t("settings.about.kv_version")}>
      {#snippet control()}
        {#if version}
          <CopyValue value={version} label={$t("settings.about.kv_version")} data-testid="about-copy-version" />
        {:else}
          <span class="text-body-sm text-muted-foreground">{$t("settings.about.version_unknown")}</span>
        {/if}
      {/snippet}
    </SettingsFieldRow>

    <SettingsFieldRow label={$t("settings.about.kv_platform")}>
      {#snippet control()}
        {#if os}
          <CopyValue value={os} label={$t("settings.about.kv_platform")} data-testid="about-copy-platform" />
        {:else}
          <span class="text-body-sm text-muted-foreground">-</span>
        {/if}
      {/snippet}
    </SettingsFieldRow>

    {#if pythonPath}
      <SettingsFieldRow label={$t("settings.about.kv_python")}>
        {#snippet control()}
          <CopyValue value={pythonPath} label={$t("settings.about.kv_python")} data-testid="about-copy-python" />
        {/snippet}
      </SettingsFieldRow>
    {/if}

    <SettingsFieldRow label={$t("settings.about.kv_engine")}>
      {#snippet control()}
        <CopyValue value={ENGINE} label={$t("settings.about.kv_engine")} data-testid="about-copy-engine" />
      {/snippet}
    </SettingsFieldRow>

    <SettingsFieldRow label={$t("settings.about.kv_stt")} border={false}>
      {#snippet control()}
        <CopyValue value={STT_ENGINE} label={$t("settings.about.kv_stt")} data-testid="about-copy-stt" />
      {/snippet}
    </SettingsFieldRow>
  {/if}

  {#snippet footer()}
    <Button
      variant="ghost"
      size="sm"
      onclick={copyDiagnostic}
      data-testid="about-copy-diagnostics"
    >
      {#snippet icon()}
        {#if diagCopied}
          <Check size={14} strokeWidth={2.2} class="text-success" />
        {:else}
          <Copy size={14} strokeWidth={1.9} />
        {/if}
      {/snippet}
      {$t("settings.about.copy_diagnostics")}
    </Button>
    <span class="ml-auto text-caption text-muted-foreground">
      {$t("settings.about.copy_hint")}
    </span>
  {/snippet}
</SettingsSection>
