<script lang="ts">
  /**
   * The persistent banner of an `apollia.toml` refused at startup: Apollia
   * runs on the default values, and says so on every screen, developer mode
   * on or off. Its action opens the editor at the faulty line; with the mode
   * off it asks first, then turns the mode on, since the editor is a section
   * of the developer mode.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { AlertTriangle } from "lucide-svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { Banner } from "$lib/components/ui/banner";
  import { Button } from "$lib/components/ui/button";
  import Dialog from "$lib/components/ui/dialog/Dialog.svelte";
  import DialogFooter from "$lib/components/ui/dialog/DialogFooter.svelte";
  import { addToast } from "$lib/components/ui/toast";
  import { readConfigFile } from "$lib/ipc/configFile";
  import { developerMode, setDeveloperMode } from "$lib/stores/developerMode";
  import { openSettingsWindow } from "$lib/stores/settingsWindow";
  import {
    checkConfigAtStartup,
    configInvalidAtStart,
    firstFaultLine,
    onConfigEvent,
    openEditorAtFault,
  } from "$lib/stores/configInvalid";

  let confirming = $state(false);
  let opening = $state(false);

  const first = $derived($configInvalidAtStart?.diagnostics[0] ?? null);
  const line = $derived($configInvalidAtStart ? firstFaultLine($configInvalidAtStart) : 1);

  async function open(): Promise<void> {
    const state = $configInvalidAtStart;
    if (!state) return;
    opening = true;
    const opened = await openEditorAtFault(state, {
      developerMode: () => $developerMode,
      turnDeveloperModeOn: () => setDeveloperMode(true),
      openSettings: (section) => openSettingsWindow(section),
    });
    opening = false;
    confirming = false;
    if (!opened) addToast($t("settings.config_invalid.mode_refused"), "error");
  }

  function onClick(): void {
    if ($developerMode) void open();
    else confirming = true;
  }

  onMount(() => {
    let disposed = false;
    let unlisten: UnlistenFn | null = null;
    void listen<{ event_type: string; payload: unknown }>("runtime-event", (event) => {
      onConfigEvent(event.payload.event_type, event.payload.payload);
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    void checkConfigAtStartup(readConfigFile);
    return () => {
      disposed = true;
      unlisten?.();
    };
  });
</script>

{#if $configInvalidAtStart}
  <Banner variant="destructive" icon={AlertTriangle} data-testid="config-invalid-banner">
    <span class="font-semibold">{$t("settings.config_invalid.title")}</span>
    {#if first}
      <span class="ml-1 opacity-90" data-testid="config-invalid-banner-detail">
        {$t("settings.config_invalid.detail", { values: { line: first.line, message: first.message } })}
      </span>
    {/if}
    {#snippet trailing()}
      <Button
        size="sm"
        variant="outline"
        onclick={onClick}
        disabled={opening}
        data-testid="config-invalid-banner-open"
      >
        {$t("settings.config_invalid.open")}
      </Button>
    {/snippet}
  </Banner>

  <Dialog
    open={confirming}
    onclose={() => (confirming = false)}
    size="sm"
    title={$t("settings.config_invalid.confirm_title")}
    data-testid="config-invalid-confirm"
  >
    <p class="text-sm text-muted-foreground">
      {$t("settings.config_invalid.confirm_message", { values: { line } })}
    </p>
    <DialogFooter>
      <Button
        variant="outline"
        onclick={() => (confirming = false)}
        data-testid="config-invalid-confirm-cancel"
      >
        {$t("common.cancel")}
      </Button>
      <Button onclick={() => void open()} disabled={opening} data-testid="config-invalid-confirm-open">
        {$t("settings.config_invalid.confirm")}
      </Button>
    </DialogFooter>
  </Dialog>
{/if}
