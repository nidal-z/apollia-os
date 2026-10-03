<script lang="ts">
  /**
   * BackgroundServiceSection - "Service without interface", in Settings >
   * Developer.
   *
   * One switch, "Keep Apollia running without the application", with its
   * explanation and a "?" explained on hover. On, the runtime is a service of
   * the session and the application attaches to it; turning it on or off
   * relaunches the application. The switch shows the service as the OS
   * reports it; the policy can only forbid installing it.
   */
  import { t } from "svelte-i18n";
  import { CircleHelp, Lock, Server } from "lucide-svelte";
  import { Toggle } from "$lib/components/ui/toggle";
  import { Tooltip } from "$lib/components/ui/tooltip";
  import { addToast } from "$lib/components/ui/toast";
  import { getBackgroundService, setBackgroundService } from "$lib/ipc/backgroundService";
  import SettingsSection from "../SettingsSection.svelte";
  import SettingsFieldRow from "../SettingsFieldRow.svelte";
  import {
    BACKGROUND_SERVICE_KEYS,
    SWITCH_OFF,
    readBackgroundService,
    type BackgroundServiceIpc,
  } from "./backgroundService";

  const ipc: BackgroundServiceIpc = {
    read: getBackgroundService,
    write: setBackgroundService,
  };

  let current = $state(SWITCH_OFF);
  let pending = $state(false);

  $effect(() => {
    let live = true;
    void readBackgroundService(ipc).then((read) => {
      if (live) current = read;
    });
    return () => {
      live = false;
    };
  });

  async function toggle(next: boolean) {
    if (pending || current.disabled) return;
    pending = true;
    try {
      // On success the application relaunches and this view goes with it.
      await ipc.write(next);
      current = await readBackgroundService(ipc);
    } catch (err: unknown) {
      const error = err instanceof Error ? err.message : String(err);
      addToast($t("settings.developer.service.write_failed", { values: { error } }), "error");
      current = await readBackgroundService(ipc);
    } finally {
      pending = false;
    }
  }
</script>

<SettingsSection
  title={$t(BACKGROUND_SERVICE_KEYS.title)}
  data-testid="developer-service-section"
>
  {#snippet icon()}<Server size={15} strokeWidth={1.75} />{/snippet}
  <SettingsFieldRow
    label={$t(BACKGROUND_SERVICE_KEYS.label)}
    hint={$t(BACKGROUND_SERVICE_KEYS.hint)}
    for="developer-service-switch"
    border={false}
    data-testid="developer-service-row"
  >
    {#snippet control()}
      <div class="flex flex-col items-end gap-1">
        <div class="inline-flex items-center gap-2">
          <Tooltip content={$t(BACKGROUND_SERVICE_KEYS.help)} side="left">
            <span
              role="img"
              class="inline-flex text-muted-foreground hover:text-foreground"
              aria-label={$t(BACKGROUND_SERVICE_KEYS.helpLabel)}
              data-testid="developer-service-help"
            >
              <CircleHelp class="h-3.5 w-3.5" aria-hidden="true" />
            </span>
          </Tooltip>
          <Toggle
            id="developer-service-switch"
            checked={current.on}
            disabled={current.disabled}
            loading={pending}
            onchange={(next) => void toggle(next)}
            aria-label={$t(BACKGROUND_SERVICE_KEYS.label)}
            data-testid="developer-service-toggle"
          />
        </div>
        {#if current.on}
          <p class="text-xs text-muted-foreground" data-testid="developer-service-state">
            {$t(
              current.running
                ? "settings.developer.service.running"
                : "settings.developer.service.stopped",
            )}
          </p>
        {/if}
        {#if current.locked}
          <p
            class="inline-flex items-center gap-1.5 text-xs text-muted-foreground"
            title={$t("settings.policy_locked_hint")}
            data-testid="developer-service-policy"
          >
            <Lock class="h-3 w-3" aria-hidden="true" />
            {$t("settings.policy_locked")}
          </p>
        {:else if current.reasonKey}
          <p class="text-xs text-muted-foreground" data-testid="developer-service-reason">
            {$t(current.reasonKey)}
          </p>
        {/if}
      </div>
    {/snippet}
  </SettingsFieldRow>
</SettingsSection>
