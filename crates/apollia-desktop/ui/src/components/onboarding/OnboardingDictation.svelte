<script lang="ts">
  /**
   * Dictation in the first launch: the settings of Settings > Dictation that
   * matter to start (the engine, its model, the language, the microphone),
   * applied in one gesture, then the same test as the settings page.
   *
   * Folded by default and never required: the person who does not dictate
   * goes on without opening it.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { Check, Mic } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Disclosure } from "$lib/components/ui/disclosure";
  import { listAudioInputDevices, reloadStt, updateSttConfig } from "$lib/ipc/stt";
  import { refreshSttStatus } from "$lib/stores/stt";
  import { settingsLoaders, sttConfigStore } from "$lib/stores/settings";
  import type { SttConfigView } from "$lib/types";
  import SttEssentialSection from "../settings/stt/SttEssentialSection.svelte";
  import SttTestCard from "../settings/stt/SttTestCard.svelte";

  interface Props {
    /** Opened from the start, when the block sits right under its model. */
    open?: boolean;
  }

  const { open = false }: Props = $props();

  let config = $state<SttConfigView | null>(null);
  let inputDevices = $state<string[]>([]);
  let devicesRead = $state(false);
  let applying = $state(false);
  let applied = $state(false);
  let failure = $state<string | null>(null);

  const noMicrophone = $derived(devicesRead && inputDevices.length === 0);

  // The form is a copy taken once: what the person types is theirs until applied.
  $effect(() => {
    const loaded = $sttConfigStore.data;
    if (loaded && !config) config = { ...loaded };
  });

  async function apply(): Promise<void> {
    if (!config || applying) return;
    applying = true;
    applied = false;
    failure = null;
    try {
      await updateSttConfig($state.snapshot(config) as SttConfigView);
      await reloadStt();
      await refreshSttStatus();
      applied = true;
    } catch (err) {
      failure = err instanceof Error ? err.message : String(err);
    } finally {
      applying = false;
    }
  }

  onMount(() => {
    void settingsLoaders.sttConfig();
    void settingsLoaders.sttModels();
    void listAudioInputDevices()
      .then((devices) => (inputDevices = devices))
      .catch(() => (inputDevices = []))
      .finally(() => (devicesRead = true));
  });
</script>

<Disclosure testid="onboarding-dictation" {open}>
  {#snippet summary()}
    <span class="flex min-w-0 flex-1 items-center gap-2">
      <Mic size={14} class="shrink-0 text-muted-foreground" aria-hidden="true" />
      <span class="text-body-sm font-medium text-foreground">{$t("onboarding_dictation.title")}</span>
      <span class="truncate text-caption text-muted-foreground">{$t("onboarding_dictation.hint")}</span>
    </span>
  {/snippet}
  <div class="flex flex-col gap-3 pt-3">
    {#if config}
      <SttEssentialSection {config} {inputDevices} {noMicrophone} />
      <div class="flex flex-wrap items-center gap-3">
        <Button
          variant="outline"
          size="sm"
          onclick={() => void apply()}
          disabled={applying}
          loading={applying}
          data-testid="onboarding-dictation-apply"
        >
          {$t("onboarding_dictation.apply")}
        </Button>
        {#if applied}
          <span class="flex items-center gap-1.5 text-caption text-success">
            <Check size={13} aria-hidden="true" />
            {$t("onboarding_dictation.applied")}
          </span>
        {/if}
        {#if failure}
          <span class="text-caption text-destructive" role="alert">{failure}</span>
        {/if}
      </div>
      <SttTestCard silenceThresholdDb={config.silence_threshold_db} disabled={!config.enabled} />
    {:else}
      <p class="text-caption text-muted-foreground">{$t("common.loading")}</p>
    {/if}
  </div>
</Disclosure>
