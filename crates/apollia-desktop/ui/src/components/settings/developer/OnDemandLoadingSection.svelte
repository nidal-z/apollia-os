<script lang="ts">
  /**
   * OnDemandLoadingSection - "On-demand loading and idle delay", in Settings >
   * Developer.
   *
   * How a model that an application names on the compatible server is loaded
   * and unloaded: loading on demand, one such model at a time, auto-unload,
   * and, under it, the idle delay. Each row reads and writes its `[server]`
   * key through the shared store, the same key the server settings show.
   */
  import { t } from "svelte-i18n";
  import { Lock, Timer } from "lucide-svelte";
  import { Input } from "$lib/components/ui/input";
  import { addToast } from "$lib/components/ui/toast";
  import {
    SERVER_SETTING_KEYS,
    serverSettings,
    setServerSetting,
    type ServerSettingKey,
  } from "$lib/stores/serverSettings";
  import SettingsSection from "../SettingsSection.svelte";
  import SettingsFieldRow from "../SettingsFieldRow.svelte";
  import SettingsToggle from "../SettingsToggle.svelte";
  import {
    ON_DEMAND_KEYS,
    ON_DEMAND_ROWS,
    idleFieldDisabled,
    parseIdleMinutes,
  } from "./onDemandLoading";

  const switches = ON_DEMAND_ROWS.filter((row) => row.key !== SERVER_SETTING_KEYS.jitIdleTtlMinutes);
  const idleRow = ON_DEMAND_ROWS[ON_DEMAND_ROWS.length - 1];

  const idle = $derived($serverSettings[SERVER_SETTING_KEYS.jitIdleTtlMinutes]);
  const idleLocked = $derived(idle.source === "policy");
  const autoUnload = $derived($serverSettings[SERVER_SETTING_KEYS.jitAutoUnload].value === true);

  let idleText = $state("");
  let idleInvalid = $state(false);

  $effect(() => {
    idleText = String(idle.value);
    idleInvalid = false;
  });

  async function write(key: ServerSettingKey, value: boolean | number) {
    const written = await setServerSetting(key, value);
    if (!written) addToast($t(ON_DEMAND_KEYS.writeFailed), "error");
  }

  function commitIdle() {
    const minutes = parseIdleMinutes(idleText);
    idleInvalid = minutes === null;
    if (minutes === null || minutes === idle.value) return;
    void write(SERVER_SETTING_KEYS.jitIdleTtlMinutes, minutes);
  }
</script>

<SettingsSection
  title={$t(ON_DEMAND_KEYS.title)}
  description={$t(ON_DEMAND_KEYS.shared)}
  bodyLayout="flush"
  data-testid="developer-on-demand"
>
  {#snippet icon()}<Timer size={15} strokeWidth={1.75} />{/snippet}
  {#each switches as row (row.key)}
    {@const setting = $serverSettings[row.key]}
    <div class="border-b border-border/60 py-1.5">
      <SettingsToggle
        id={`${row.testid}-switch`}
        label={$t(row.label)}
        description={$t(row.hint)}
        checked={setting.value === true}
        source={setting.source}
        onChange={(next) => void write(row.key, next)}
        data-testid={row.testid}
      />
    </div>
    {#if row.key === SERVER_SETTING_KEYS.jitAutoUnload}
      <div class="pl-6">
        <SettingsFieldRow
          label={$t(idleRow.label)}
          hint={$t(idleRow.hint)}
          for="developer-jit-idle-input"
          border={false}
          data-testid={idleRow.testid}
        >
          {#snippet control()}
            <div class="flex flex-col items-end gap-1">
              <div class="inline-flex items-center gap-2">
                <Input
                  id="developer-jit-idle-input"
                  size="sm"
                  inputmode="numeric"
                  class="w-20 text-right"
                  bind:value={idleText}
                  disabled={idleFieldDisabled(autoUnload, idleLocked)}
                  aria-invalid={idleInvalid}
                  aria-label={$t(idleRow.label)}
                  onblur={commitIdle}
                  onkeydown={(e: KeyboardEvent) => {
                    if (e.key === "Enter") commitIdle();
                  }}
                  data-testid="developer-jit-idle-input"
                />
                <span class="text-xs text-muted-foreground">{$t(ON_DEMAND_KEYS.unit)}</span>
              </div>
              {#if idleInvalid}
                <p class="text-xs text-destructive" data-testid="developer-jit-idle-invalid">
                  {$t(ON_DEMAND_KEYS.invalid)}
                </p>
              {/if}
              {#if idleLocked}
                <p
                  class="inline-flex items-center gap-1.5 text-xs text-muted-foreground"
                  title={$t("settings.policy_locked_hint")}
                  data-testid="developer-jit-idle-policy"
                >
                  <Lock class="h-3 w-3" aria-hidden="true" />
                  {$t("settings.policy_locked")}
                </p>
              {/if}
            </div>
          {/snippet}
        </SettingsFieldRow>
      </div>
    {/if}
  {/each}
</SettingsSection>
