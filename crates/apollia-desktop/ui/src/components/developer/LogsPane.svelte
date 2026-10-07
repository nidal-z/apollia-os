<script lang="ts">
  /**
   * The logs of the local server: each line with its time and its level, the
   * level asked of the server, "Clear", and the fold. On the developer screen
   * the pane sits under the local server and can hand the logs to a window of
   * their own; `windowed` is that window, which hands them back.
   *
   * The logs live in the runtime: the pane reads their stream and holds
   * nothing of them. Clearing empties them there, and nothing else.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { ChevronDown, ChevronUp, PictureInPicture2, Trash2, Undo2 } from "lucide-svelte";
  import { Banner } from "$lib/components/ui/banner";
  import { Button } from "$lib/components/ui/button";
  import { Select } from "$lib/components/ui/select";
  import { cn } from "$lib/utils";
  import { developerMode } from "$lib/stores/developerMode";
  import {
    clearServerLogs,
    closeServerLogsWindow,
    listenServerLogLines,
    listenServerLogsClosed,
    listenServerLogsReattached,
    openServerLogsWindow,
    unwatchServerLogs,
    watchServerLogs,
    type ServerLogLevel,
  } from "$lib/ipc/serverLogs";
  import LogLines from "./LogLines.svelte";
  import { LEVEL_LABELS, LOG_LEVELS, createLogsView, type ServerLogsIpc } from "./serverLogs";

  interface Props {
    /** The view of the detached window: never folded, never detached itself. */
    windowed?: boolean;
  }

  let { windowed = false }: Props = $props();

  const ipc: ServerLogsIpc = {
    watch: watchServerLogs,
    unwatch: unwatchServerLogs,
    clear: clearServerLogs,
    openWindow: openServerLogsWindow,
    closeWindow: closeServerLogsWindow,
    onLines: listenServerLogLines,
    onClosed: listenServerLogsClosed,
    onReattached: listenServerLogsReattached,
  };

  // svelte-ignore state_referenced_locally
  const logs = createLogsView(ipc, { windowed });

  const shown = $derived(!$logs.collapsed && !$logs.detached);

  onMount(() => {
    void logs.open();
    return () => void logs.close();
  });

  function onlevel(event: Event): void {
    const level = (event.currentTarget as HTMLSelectElement).value as ServerLogLevel;
    void logs.setLevel(level);
  }
</script>

{#if $developerMode}
  <section
    class={cn(
      "flex min-h-0 flex-col border-t border-border bg-background",
      windowed ? "h-full flex-1" : shown ? "h-72 shrink-0" : "shrink-0",
    )}
    aria-labelledby="developer-logs-title"
    data-testid="developer-pane-logs"
    data-status={$logs.status}
    data-collapsed={$logs.collapsed}
    data-detached={$logs.detached}
  >
    <header class="flex shrink-0 flex-wrap items-center gap-2 px-4 py-2">
      <h2 id="developer-logs-title" class="text-sm font-medium text-foreground">
        {$t("dev.developer.logs.title")}
      </h2>
      {#if !$logs.detached}
        <span class="text-xs text-muted-foreground" data-testid="developer-logs-summary">
          {$t("dev.developer.logs.summary", { values: { count: $logs.lines.length } })}
        </span>
      {/if}

      <div class="ml-auto flex items-center gap-1">
        {#if $logs.detached}
          <span class="text-xs text-muted-foreground" data-testid="developer-logs-detached">
            {$t("dev.developer.logs.detached_notice")}
          </span>
          <Button
            variant="ghost"
            size="sm"
            onclick={() => void logs.reattach()}
            data-testid="developer-logs-reattach"
          >
            {#snippet icon()}<Undo2 size={14} aria-hidden="true" />{/snippet}
            {$t("dev.developer.logs.reattach")}
          </Button>
        {:else}
          {#if shown}
            <div class="w-52">
              <Select
                size="sm"
                value={$logs.level}
                onchange={onlevel}
                aria-label={$t("dev.developer.logs.level_label")}
                data-testid="developer-logs-level"
              >
                {#each LOG_LEVELS as level (level)}
                  <option value={level}>{$t(LEVEL_LABELS[level])}</option>
                {/each}
              </Select>
            </div>
            <Button
              variant="ghost"
              size="sm"
              title={$t("dev.developer.logs.clear_title")}
              onclick={() => void logs.clear()}
              data-testid="developer-logs-clear"
            >
              {#snippet icon()}<Trash2 size={14} aria-hidden="true" />{/snippet}
              {$t("dev.developer.logs.clear")}
            </Button>
          {/if}
          {#if windowed}
            <Button
              variant="ghost"
              size="sm"
              onclick={() => void logs.reattach()}
              data-testid="developer-logs-reattach"
            >
              {#snippet icon()}<Undo2 size={14} aria-hidden="true" />{/snippet}
              {$t("dev.developer.logs.reattach")}
            </Button>
          {:else}
            <Button
              variant="ghost"
              size="icon-sm"
              title={$t("dev.developer.logs.detach")}
              aria-label={$t("dev.developer.logs.detach")}
              onclick={() => void logs.detach()}
              data-testid="developer-logs-detach"
            >
              <PictureInPicture2 size={14} aria-hidden="true" />
            </Button>
            <Button
              variant="ghost"
              size="icon-sm"
              title={$t($logs.collapsed ? "dev.developer.logs.expand" : "dev.developer.logs.collapse")}
              aria-label={$t(
                $logs.collapsed ? "dev.developer.logs.expand" : "dev.developer.logs.collapse",
              )}
              aria-expanded={!$logs.collapsed}
              onclick={() => logs.toggleCollapsed()}
              data-testid="developer-logs-collapse"
            >
              {#if $logs.collapsed}
                <ChevronUp size={14} aria-hidden="true" />
              {:else}
                <ChevronDown size={14} aria-hidden="true" />
              {/if}
            </Button>
          {/if}
        {/if}
      </div>
    </header>

    {#if $logs.error}
      <div class="px-4 pb-2">
        <Banner variant="destructive" surface="card" data-testid="developer-logs-error">
          {$t("dev.developer.logs.failed", { values: { error: $logs.error } })}
        </Banner>
      </div>
    {/if}

    {#if shown}
      {#if $logs.status === "interrupted" || $logs.status === "failed"}
        <div
          class="flex items-center gap-2 px-4 pb-2 text-xs text-muted-foreground"
          data-testid="developer-logs-interrupted"
        >
          <span>{$t("dev.developer.logs.interrupted")}</span>
          <Button
            variant="outline"
            size="sm"
            onclick={() => void logs.resume()}
            data-testid="developer-logs-resume"
          >
            {$t("dev.developer.logs.resume")}
          </Button>
        </div>
      {/if}
      <LogLines lines={$logs.lines} />
    {/if}
  </section>
{/if}
