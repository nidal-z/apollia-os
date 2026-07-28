<script lang="ts">
  /**
   * HardwareCard - detected-hardware section of the Model Hub.
   *
   * Renders the RAM / accelerator / inference-budget grid derived from
   * `get_hardware_profile`. The budget governs the per-variant fit verdict.
   */
  import { t } from "svelte-i18n";
  import { Cpu, Sparkles } from "lucide-svelte";
  import { Spinner } from "$lib/components/ui/progress";
  import { Badge } from "$lib/components/ui/badge";
  import SettingsSection from "../SettingsSection.svelte";
  import type { HardwareProfile } from "$lib/ipc/modelHub";
  import { chipLabel, formatBudget } from "./helpers";

  interface Props {
    hardware: HardwareProfile | null;
    loading: boolean;
  }

  let { hardware, loading }: Props = $props();
</script>

<SettingsSection
  title={$t("settings.model_hub.hardware.label")}
  description={$t("settings.model_hub.hardware.help")}
  data-testid="model-hardware-card"
>
  {#snippet icon()}<Cpu size={15} strokeWidth={1.75} />{/snippet}
  {#snippet actions()}
    {#if hardware}
      <Badge variant="primary" size="md">
        {#snippet icon()}<Sparkles size={11} aria-hidden="true" />{/snippet}
        {chipLabel(hardware.accelerator, $t("settings.model_hub.hardware.cpu_only"))}
      </Badge>
    {/if}
  {/snippet}

  {#if loading}
    <div class="flex items-center gap-2 text-body-sm text-muted-foreground">
      <Spinner class="h-4 w-4" />
      {$t("settings.model_hub.hardware.detecting")}
    </div>
  {:else if hardware}
    <div class="grid grid-cols-2 gap-x-8 gap-y-3 sm:grid-cols-4">
      <div>
        <div class="text-body-sm text-muted-foreground">{$t("settings.model_hub.hardware.ram")}</div>
        <div class="text-body-md font-semibold tabular-nums text-foreground">{hardware.total_ram_gb.toFixed(0)} GB</div>
      </div>
      <div>
        <div class="text-body-sm text-muted-foreground">{$t("settings.model_hub.hardware.available")}</div>
        <div class="text-body-md font-semibold tabular-nums text-foreground">{hardware.available_ram_gb.toFixed(1)} GB</div>
      </div>
      <div>
        <div class="text-body-sm text-muted-foreground">{$t("settings.model_hub.hardware.gpu")}</div>
        <div class="text-body-md font-semibold text-foreground">{chipLabel(hardware.accelerator, $t("settings.model_hub.hardware.cpu_only"))}</div>
      </div>
      <div>
        <div class="text-body-sm text-muted-foreground">{$t("settings.model_hub.hardware.budget")}</div>
        <div class="text-body-md font-semibold tabular-nums text-success-a11y">{formatBudget(hardware.memory_budget_gb)}</div>
      </div>
    </div>
    <p class="text-caption text-muted-foreground/80">
      {$t("settings.model_hub.hardware.cpu_info", { values: { cpu: hardware.cpu_model, cores: hardware.cpu_cores } })}
    </p>
  {/if}
</SettingsSection>
