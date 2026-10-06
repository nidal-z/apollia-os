<script lang="ts">
  /**
   * The bar of the local server: whether it runs, the switch that starts and
   * stops it (and its shortcut), the address a client reaches it at with its
   * copy button, and "Load a model". A start refused because another program
   * holds the port says so under the bar.
   *
   * The server lives in the runtime: the bar reads and drives it, and holds
   * nothing of it.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { Check, Copy, TriangleAlert } from "lucide-svelte";
  import { Banner } from "$lib/components/ui/banner";
  import { Toggle } from "$lib/components/ui/toggle";
  import { Button } from "$lib/components/ui/button";
  import { addToast } from "$lib/components/ui/toast";
  import KeyboardHint from "$lib/components/feedback/KeyboardHint.svelte";
  import { developerMode } from "$lib/stores/developerMode";
  import { getLocalServer, startLocalServer, stopLocalServer } from "$lib/ipc/localServer";
  import { SHORTCUTS, comboFor, splitCombo } from "$lib/navigation/shortcuts";
  import { registerShortcutHandler } from "$lib/navigation/shortcutDispatcher";
  import LoadModelButton from "./LoadModelButton.svelte";
  import {
    EMPTY_BAR,
    SERVER_TOGGLE_SHORTCUT,
    STATUS_LABELS,
    copyAddress,
    nextSwitchPosition,
    reachableAddress,
    readServer,
    serverStatus,
    switchServer,
    type LocalServerIpc,
    type ServerBarState,
  } from "./localServer";

  const ipc: LocalServerIpc = {
    read: getLocalServer,
    start: startLocalServer,
    stop: stopLocalServer,
  };

  const shortcut = SHORTCUTS.find((entry) => entry.id === SERVER_TOGGLE_SHORTCUT);
  const keys = shortcut ? splitCombo(comboFor(shortcut)) : [];

  let bar = $state<ServerBarState>(EMPTY_BAR);
  let switching = $state(false);
  // What the switch shows: the position asked for while a gesture runs, the
  // state read once it answered. A refused start brings it back to off.
  let on = $state(false);
  let copied = $state(false);

  const status = $derived(serverStatus(bar));
  const address = $derived(reachableAddress(bar));
  const failure = $derived(bar.failure);

  onMount(() => {
    let live = true;
    void readServer(ipc).then((read) => {
      if (!live) return;
      bar = read;
      on = serverStatus(read) === "running";
    });
    const unregister = registerShortcutHandler({
      id: SERVER_TOGGLE_SHORTCUT,
      handler: () => void flip(nextSwitchPosition(bar)),
    });
    return () => {
      live = false;
      unregister();
    };
  });

  async function flip(next: boolean): Promise<void> {
    if (switching) return;
    switching = true;
    on = next;
    bar = await switchServer(ipc, bar, next);
    on = serverStatus(bar) === "running";
    switching = false;
  }

  async function copy(): Promise<void> {
    const done = await copyAddress(bar, (text) => navigator.clipboard.writeText(text));
    if (!done) {
      addToast($t("dev.developer.server.copy_failed"), "error");
      return;
    }
    copied = true;
    setTimeout(() => (copied = false), 2000);
  }
</script>

{#if $developerMode}
  <div class="flex flex-col gap-2" data-testid="developer-server-bar">
    <div
      class="flex flex-wrap items-center gap-3 rounded-lg border border-border bg-card px-3 py-2"
    >
      <div
        class="flex items-center gap-2 rounded-md border border-border px-3 py-1.5"
        data-testid="developer-server-status"
        data-status={status}
      >
        {#if status === "error"}
          <TriangleAlert class="h-4 w-4 text-destructive" aria-hidden="true" />
        {/if}
        <span class="text-sm text-foreground" data-testid="developer-server-status-label">
          {$t(STATUS_LABELS[status])}
        </span>
        <Toggle
          checked={on}
          loading={switching}
          disabled={bar.view === null}
          onchange={(next) => void flip(next)}
          aria-label={$t("dev.developer.server.switch_label")}
          data-testid="developer-server-toggle"
        />
        {#if keys.length > 0}
          <KeyboardHint {keys} aria-label={$t("dev.developer.server.shortcut_label")} />
        {/if}
      </div>

      <div class="ml-auto flex items-center gap-2">
        {#if address}
          <span class="text-xs text-success-a11y">{$t("dev.developer.server.reachable_at")}</span>
          <code
            class="rounded-md border border-border bg-muted px-2 py-1 font-mono text-xs text-foreground"
            data-testid="developer-server-address">{address}</code
          >
          <Button
            variant="ghost"
            size="icon-sm"
            title={$t(copied ? "dev.developer.server.copied" : "dev.developer.server.copy")}
            aria-label={$t("dev.developer.server.copy")}
            onclick={() => void copy()}
            data-testid="developer-server-copy"
          >
            {#if copied}
              <Check size={14} aria-hidden="true" />
            {:else}
              <Copy size={14} aria-hidden="true" />
            {/if}
          </Button>
        {:else}
          <span class="text-xs text-muted-foreground" data-testid="developer-server-no-address">
            {$t(
              status === "stopped"
                ? "dev.developer.server.no_address_stopped"
                : "dev.developer.server.no_address",
            )}
          </span>
        {/if}
        <LoadModelButton />
      </div>
    </div>

    {#if failure}
      <Banner variant="destructive" surface="card" data-testid="developer-server-error">
        <span data-testid={`developer-server-error-${failure.kind}`}>
          {#if failure.kind === "port_in_use"}
            {$t("dev.developer.server.port_in_use", { values: { port: failure.port } })}
          {:else if failure.kind === "unreadable"}
            {$t("dev.developer.server.unreadable", { values: { error: failure.message } })}
          {:else}
            {$t("dev.developer.server.failed", { values: { error: failure.message } })}
          {/if}
        </span>
      </Banner>
    {/if}
  </div>
{/if}
