<script lang="ts" module>
  import type { AcceleratorProfile } from "$lib/ipc/modelHub";

  /** Display name of a processor architecture as Rust names it. */
  export function archLabel(arch: string | null | undefined): string | null {
    if (!arch) return null;
    if (arch === "aarch64") return "ARM64";
    if (arch === "x86_64") return "x86-64";
    return arch;
  }

  /**
   * How the accelerator reaches its memory: shared with the processor on Apple
   * Silicon, its own on a card that reports some, and nothing to say otherwise.
   */
  export function memoryKind(acc: AcceleratorProfile): "unified" | "dedicated" | null {
    if (acc.kind === "apple_silicon") return "unified";
    if ((acc.kind === "cuda" || acc.kind === "generic") && (acc.vram_gb ?? 0) > 0) {
      return "dedicated";
    }
    return null;
  }

  /** Video memory the probe reports, `null` without a GPU or when it reports none. */
  export function vramGb(acc: AcceleratorProfile): number | null {
    if (acc.kind === "none") return null;
    return acc.vram_gb && acc.vram_gb > 0 ? acc.vram_gb : null;
  }

  /** The backend the engine drives the GPU with. */
  export function gpuBackend(acc: AcceleratorProfile): string | null {
    if (acc.kind === "apple_silicon") return "Metal";
    if (acc.kind === "cuda") return "CUDA";
    if (acc.kind === "generic") return "Vulkan";
    return null;
  }
</script>

<script lang="ts">
  /**
   * HardwareCard - System > Hardware: the processor, the memory and the GPU,
   * all read from the single hardware probe the inference sizes models with
   * (`get_hardware_profile`). Nothing here probes on its own.
   */
  import { t } from "svelte-i18n";
  import { Cpu, MemoryStick, MonitorCog } from "lucide-svelte";
  import { Spinner } from "$lib/components/ui/progress";
  import { Badge } from "$lib/components/ui/badge";
  import { ErrorBanner } from "$lib/components/operator";
  import SettingsSection from "../SettingsSection.svelte";
  import SettingsFieldRow from "../SettingsFieldRow.svelte";
  import type { HardwareProfile } from "$lib/ipc/modelHub";
  import { chipLabel } from "../model-hub/helpers";

  interface Props {
    hardware: HardwareProfile | null;
    loading: boolean;
    error: string | null;
    /** Processor architecture this build runs on, from the system information. */
    arch: string | null;
  }

  let { hardware, loading, error, arch }: Props = $props();

  const gb = (value: number): string =>
    $t("settings.hardware.gb", { values: { n: value.toFixed(value >= 100 ? 0 : 1) } });
</script>

{#if loading && !hardware}
  <div
    class="flex items-center gap-2 text-body-sm text-muted-foreground"
    data-testid="hardware-detecting"
  >
    <Spinner class="h-4 w-4" />
    {$t("settings.hardware.detecting")}
  </div>
{:else if hardware}
  {@const kind = memoryKind(hardware.accelerator)}
  {@const vram = vramGb(hardware.accelerator)}
  {@const backend = gpuBackend(hardware.accelerator)}
  <div class="grid gap-4 md:grid-cols-2">
    <SettingsSection title={$t("settings.hardware.processor")} data-testid="hardware-processor">
      {#snippet icon()}<Cpu size={15} strokeWidth={1.75} />{/snippet}
      {#snippet actions()}
        <!-- The build that renders this page runs on this processor. -->
        <Badge variant="success" size="sm" data-testid="hardware-processor-compatible">
          {$t("settings.hardware.compatible")}
        </Badge>
      {/snippet}
      <div>
        <SettingsFieldRow label={$t("settings.hardware.cpu_name")}>
          {#snippet control()}
            <span class="text-body-sm text-foreground" data-testid="hardware-cpu-name">
              {hardware.cpu_model}
            </span>
          {/snippet}
        </SettingsFieldRow>
        <SettingsFieldRow label={$t("settings.hardware.architecture")}>
          {#snippet control()}
            <span class="text-body-sm text-foreground" data-testid="hardware-cpu-arch">
              {archLabel(arch) ?? "-"}
            </span>
          {/snippet}
        </SettingsFieldRow>
        <SettingsFieldRow label={$t("settings.hardware.cores")} border={false}>
          {#snippet control()}
            <span class="text-body-sm tabular-nums text-foreground">{hardware.cpu_cores}</span>
          {/snippet}
        </SettingsFieldRow>
      </div>
    </SettingsSection>

    <SettingsSection
      title={$t("settings.hardware.memory")}
      description={kind === "unified"
        ? $t("settings.hardware.memory_unified")
        : kind === "dedicated"
          ? $t("settings.hardware.memory_dedicated")
          : undefined}
      data-testid="hardware-memory"
    >
      {#snippet icon()}<MemoryStick size={15} strokeWidth={1.75} />{/snippet}
      <div>
        <SettingsFieldRow label={$t("settings.hardware.ram")}>
          {#snippet control()}
            <span class="text-body-sm tabular-nums text-foreground" data-testid="hardware-ram">
              {gb(hardware.total_ram_gb)}
            </span>
          {/snippet}
        </SettingsFieldRow>
        <SettingsFieldRow label={$t("settings.hardware.vram")} border={false}>
          {#snippet control()}
            <span class="text-body-sm tabular-nums text-foreground" data-testid="hardware-vram">
              {vram !== null ? gb(vram) : $t("settings.hardware.none")}
            </span>
          {/snippet}
        </SettingsFieldRow>
      </div>
    </SettingsSection>
  </div>

  <SettingsSection title={$t("settings.hardware.gpu")} data-testid="hardware-gpu">
    {#snippet icon()}<MonitorCog size={15} strokeWidth={1.75} />{/snippet}
    {#if backend}
      <div>
        <p class="text-body-sm font-medium text-foreground" data-testid="hardware-gpu-count">
          {$t("settings.hardware.gpu_detected", { values: { backend } })}
        </p>
        <p class="text-body-sm text-foreground" data-testid="hardware-gpu-name">
          {chipLabel(hardware.accelerator, $t("settings.hardware.no_gpu"))}
        </p>
        {#if vram !== null}
          <p class="text-caption text-muted-foreground">
            {$t("settings.hardware.gpu_vram", { values: { size: gb(vram), backend } })}
          </p>
        {/if}
      </div>
    {:else}
      <p class="text-body-sm text-muted-foreground" data-testid="hardware-gpu-none">
        {$t("settings.hardware.no_gpu")}
      </p>
    {/if}
  </SettingsSection>
{:else if error}
  <ErrorBanner message={error} data-testid="hardware-error" />
{/if}
