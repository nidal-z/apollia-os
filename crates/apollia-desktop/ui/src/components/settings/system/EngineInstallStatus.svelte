<script lang="ts">
  /**
   * EngineInstallStatus - where an installation started from the Engines page
   * stands, on the line it was started from: the signature verified, the
   * download with its progress, the installation, its end, or why the engine
   * store stopped it.
   */
  import { t } from "svelte-i18n";
  import { ProgressBar, Spinner } from "$lib/components/ui/progress";
  import type { EngineInstallation } from "$lib/ipc/engineInstallation";
  import { REFUSAL_BODY_KEYS, refusalReason } from "../../ai/acceleratedEngine";
  import { downloadPercent } from "$lib/settings/engineManagement";

  interface Props {
    installation: EngineInstallation;
    "data-testid": string;
  }

  let { installation, "data-testid": testid }: Props = $props();

  const percent = $derived(downloadPercent(installation));

  /** The sentence of each step that is not the download. */
  const STEP_KEYS = {
    verifying: "settings.engines.install_progress.verifying",
    installing: "settings.engines.install_progress.installing",
    installed: "settings.engines.install_progress.installed",
  } as const;
</script>

{#if installation.state === "failed"}
  <p class="text-caption text-destructive" role="alert" data-testid="{testid}-error">
    {$t(
      installation.kind === "import"
        ? "settings.engines.import_refused"
        : "settings.engines.install_refused",
      {
        values: {
          reason: $t(REFUSAL_BODY_KEYS[refusalReason(installation.kind, installation.code)]),
        },
      },
    )}
  </p>
{:else if installation.state !== "idle"}
  <div class="flex flex-col gap-1" role="status" data-testid={testid}>
    <p class="flex items-center gap-2 text-caption text-muted-foreground">
      {#if installation.state !== "installed"}<Spinner size={12} />{/if}
      {#if installation.state === "downloading"}
        {percent === null
          ? $t("settings.engines.install_progress.downloading_unknown")
          : $t("settings.engines.install_progress.downloading", { values: { percent } })}
      {:else}
        {$t(STEP_KEYS[installation.state])}
      {/if}
    </p>
    {#if installation.state === "downloading"}
      <ProgressBar value={percent ?? undefined} size="sm" />
    {/if}
  </div>
{/if}
