<script lang="ts">
  /**
   * The lines of the logs of the server: one row per line, its time, its
   * level and its text. The view follows the end while the reader stays
   * there, and stays put once they scroll up.
   */
  import { tick } from "svelte";
  import { t } from "svelte-i18n";
  import { cn } from "$lib/utils";
  import { developerMode } from "$lib/stores/developerMode";
  import type { ServerLogLine } from "$lib/ipc/serverLogs";
  import { followsEnd, lineLevel, lineText, lineTime, scrollAfterLines } from "./serverLogs";

  interface Props {
    lines: ServerLogLine[];
  }

  let { lines }: Props = $props();

  let viewport = $state<HTMLDivElement | null>(null);
  let following = true;

  const LEVEL_CLASSES: Record<ServerLogLine["level"], string> = {
    debug: "text-muted-foreground",
    info: "text-foreground",
    warn: "text-warning-a11y",
    error: "text-danger-a11y",
  };

  function onscroll(): void {
    if (!viewport) return;
    following = followsEnd(viewport.scrollTop, viewport.clientHeight, viewport.scrollHeight);
  }

  $effect(() => {
    // Read so the effect runs for each batch of lines.
    void lines.length;
    const node = viewport;
    if (!node) return;
    void tick().then(() => {
      node.scrollTop = scrollAfterLines(following, node.scrollTop, node.scrollHeight);
    });
  });
</script>

{#if $developerMode}
  <div
    bind:this={viewport}
    class="min-h-0 flex-1 overflow-y-auto bg-muted px-4 py-2 font-mono text-xs"
    role="log"
    aria-label={$t("dev.developer.logs.title")}
    {onscroll}
    data-testid="developer-logs-lines"
    data-count={lines.length}
  >
    {#if lines.length === 0}
      <p class="font-sans text-muted-foreground" data-testid="developer-logs-empty">
        {$t("dev.developer.logs.empty")}
      </p>
    {:else}
      {#each lines as line (line.seq)}
        <div class="flex gap-3 py-0.5" data-level={line.level}>
          <time class="shrink-0 text-muted-foreground" datetime={line.timestamp}>
            {lineTime(line.timestamp)}
          </time>
          <span class={cn("w-12 shrink-0 font-medium", LEVEL_CLASSES[line.level])}>
            {lineLevel(line)}
          </span>
          <span class="min-w-0 whitespace-pre-wrap break-words text-foreground">
            {lineText(line)}
          </span>
        </div>
      {/each}
    {/if}
  </div>
{/if}
