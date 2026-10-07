<script lang="ts">
  /**
   * "Server settings": the panel the bar of the local server opens, with
   * every setting of the server in one column, each explained on hover of
   * its "?". A row reads and writes its key of `[server]` through the shared
   * store; a setting the listener reads at its opening reopens a running
   * server. A port that is not one, or that another program holds, is said
   * under the port, and the panel stays open.
   */
  import { t } from "svelte-i18n";
  import { Settings2 } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Popover } from "$lib/components/ui/popover";
  import { Toggle } from "$lib/components/ui/toggle";
  import { developerMode } from "$lib/stores/developerMode";
  import { getLocalServer, restartLocalServer, type LocalServerView } from "$lib/ipc/localServer";
  import {
    SERVER_SETTING_KEYS,
    hydrateServerSettings,
    serverSettings,
    setServerSetting,
    type ServerSettingKey,
  } from "$lib/stores/serverSettings";
  import ServerSettingRow from "./ServerSettingRow.svelte";
  import type { ServerFailure } from "./localServer";
  import {
    SERVER_SETTINGS_TEXTS,
    SERVER_SETTING_ROWS,
    barFailure,
    changeMinutes,
    changePort,
    changeServerSetting,
    imposedByPolicy,
    rowDisabled,
    settingTestId,
    type SettingChange,
    type SettingChangeDeps,
    type SettingFailure,
  } from "./serverSettings";

  interface Props {
    /** The state of the server as the bar last read it. */
    view: LocalServerView | null;
    /** A reopening changed the state of the server: the bar takes it. */
    onserver: (view: LocalServerView | null, failure: ServerFailure | null) => void;
    /** Opens the window of the tokens; the button waits while there is none. */
    onmanagetokens?: () => void;
  }

  let { view, onserver, onmanagetokens }: Props = $props();

  let open = $state(false);
  let busy = $state<ServerSettingKey | null>(null);
  let failure = $state<SettingFailure | null>(null);
  let portText = $state("");
  let minutesText = $state("");

  const port = $derived($serverSettings[SERVER_SETTING_KEYS.port].value);
  const minutes = $derived($serverSettings[SERVER_SETTING_KEYS.jitIdleTtlMinutes].value);

  $effect(() => {
    portText = String(port);
  });
  $effect(() => {
    minutesText = String(minutes);
  });
  $effect(() => {
    if (!open) return;
    failure = null;
    void hydrateServerSettings();
  });

  function deps(): SettingChangeDeps {
    return { write: setServerSetting, restart: restartLocalServer, read: getLocalServer, view };
  }

  async function run(key: ServerSettingKey, change: () => Promise<SettingChange>): Promise<void> {
    if (busy !== null) return;
    busy = key;
    const done = await change();
    failure = done.failure;
    if (done.view !== undefined) onserver(done.view, barFailure(done.failure));
    busy = null;
  }

  function flip(key: ServerSettingKey, next: boolean): void {
    void run(key, () => changeServerSetting(key, next, deps()));
  }

  function commitPort(): void {
    void run(SERVER_SETTING_KEYS.port, () => changePort(portText, Number(port), deps()));
  }

  function commitMinutes(): void {
    void run(SERVER_SETTING_KEYS.jitIdleTtlMinutes, () =>
      changeMinutes(minutesText, Number(minutes), deps()),
    );
  }

  function onEnter(event: KeyboardEvent, commit: () => void): void {
    if (event.key === "Enter") commit();
  }

  const portFailed = $derived(failure?.kind === "port_invalid" || failure?.kind === "port_taken");
</script>

{#if $developerMode}
  <Popover bind:open align="start" class="w-[26rem] max-w-[90vw] p-0">
    {#snippet trigger(triggerProps)}
      <Button
        {...triggerProps}
        variant="outline"
        size="sm"
        aria-expanded={open}
        data-testid="developer-server-settings-open"
      >
        {#snippet icon()}<Settings2 size={14} aria-hidden="true" />{/snippet}
        {$t(SERVER_SETTINGS_TEXTS.open)}
      </Button>
    {/snippet}
    {#snippet content()}
      <div
        class="flex max-h-[70vh] flex-col overflow-y-auto"
        role="group"
        aria-label={$t(SERVER_SETTINGS_TEXTS.title)}
        data-testid="developer-server-settings"
      >
        {#each SERVER_SETTING_ROWS as item (item.key)}
          {@const setting = $serverSettings[item.key]}
          {@const locked = imposedByPolicy(item.key, setting, view)}
          {@const disabled = rowDisabled(item, $serverSettings, view)}
          {@const testid = settingTestId(item)}
          <ServerSettingRow
            label={$t(item.label)}
            help={$t(item.help)}
            {testid}
            for={`${testid}-control`}
            {locked}
            dimmed={disabled}
            under={item.under !== undefined}
          >
            {#snippet control()}
              {#if item.control === "port"}
                <Input
                  id={`${testid}-control`}
                  size="sm"
                  inputmode="numeric"
                  class="w-20 text-right font-mono"
                  bind:value={portText}
                  {disabled}
                  aria-invalid={portFailed}
                  onblur={commitPort}
                  onkeydown={(event: KeyboardEvent) => onEnter(event, commitPort)}
                  data-testid={`${testid}-control`}
                />
              {:else if item.control === "minutes"}
                <Input
                  id={`${testid}-control`}
                  size="sm"
                  inputmode="numeric"
                  class="w-16 text-right"
                  bind:value={minutesText}
                  {disabled}
                  aria-invalid={failure?.kind === "minutes_invalid"}
                  onblur={commitMinutes}
                  onkeydown={(event: KeyboardEvent) => onEnter(event, commitMinutes)}
                  data-testid={`${testid}-control`}
                />
                <span class="text-xs text-muted-foreground">{$t(SERVER_SETTINGS_TEXTS.minutes)}</span>
              {:else}
                <Toggle
                  id={`${testid}-control`}
                  checked={setting.value === true}
                  {disabled}
                  loading={busy === item.key}
                  onchange={(next) => flip(item.key, next)}
                  aria-label={$t(item.label)}
                  data-testid={`${testid}-control`}
                />
              {/if}
            {/snippet}
            {#snippet detail()}
              {#if item.control === "port" && failure?.kind === "port_invalid"}
                <p class="text-xs text-destructive" role="alert" data-testid="developer-server-port-invalid">
                  {$t(SERVER_SETTINGS_TEXTS.portInvalid)}
                </p>
              {:else if item.control === "port" && failure?.kind === "port_taken"}
                <p class="text-xs text-destructive" role="alert" data-testid="developer-server-port-taken">
                  {$t(SERVER_SETTINGS_TEXTS.portTaken, { values: { port: failure.port } })}
                </p>
              {:else if item.control === "minutes" && failure?.kind === "minutes_invalid"}
                <p class="text-xs text-destructive" role="alert" data-testid="developer-server-minutes-invalid">
                  {$t(SERVER_SETTINGS_TEXTS.minutesInvalid)}
                </p>
              {:else if item.key === SERVER_SETTING_KEYS.requireAuth}
                <div class="flex items-center justify-between gap-3">
                  <span class="text-xs text-muted-foreground" data-testid="developer-server-active-tokens">
                    {$t(SERVER_SETTINGS_TEXTS.activeTokens, { values: { count: view?.activeTokens ?? 0 } })}
                  </span>
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={onmanagetokens === undefined}
                    onclick={() => onmanagetokens?.()}
                    data-testid="developer-server-manage-tokens"
                  >
                    {$t(SERVER_SETTINGS_TEXTS.manageTokens)}
                  </Button>
                </div>
              {/if}
            {/snippet}
          </ServerSettingRow>
        {/each}
        {#if failure?.kind === "write" || failure?.kind === "reopen"}
          <p
            class="border-t border-border/60 px-3 py-2 text-xs text-destructive"
            role="alert"
            data-testid={`developer-server-settings-error-${failure.kind}`}
          >
            {#if failure.kind === "write"}
              {$t(SERVER_SETTINGS_TEXTS.writeFailed)}
            {:else}
              {$t(SERVER_SETTINGS_TEXTS.reopenFailed, { values: { error: failure.message } })}
            {/if}
          </p>
        {/if}
      </div>
    {/snippet}
  </Popover>
{/if}
