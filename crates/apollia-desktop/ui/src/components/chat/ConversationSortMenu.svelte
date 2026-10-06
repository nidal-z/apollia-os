<!--
  Sort menu of the conversation list: one choice that carries both the
  criterion and the direction (newest or oldest created first, most recent
  activity first, name A to Z). The choice is remembered (`sessionSort`).
-->
<script module lang="ts">
  import type { SessionSort } from "$lib/stores/chatSessions";

  /** The i18n key of each choice. */
  export const SESSION_SORT_LABEL_KEYS: Record<SessionSort, string> = {
    newest: "chat.sort.newest",
    oldest: "chat.sort.oldest",
    activity: "chat.sort.activity",
    name: "chat.sort.name",
  };
</script>

<script lang="ts">
  import { t } from "svelte-i18n";
  import { ArrowUpDown, Check } from "lucide-svelte";
  import { ActionMenu } from "$lib/components/ui/action-menu";
  import { SESSION_SORTS, sessionSort } from "$lib/stores/chatSessions";
  import { cn } from "$lib/utils";
</script>

<ActionMenu triggerLabel={$t("chat.sort.trigger")} align="start">
  {#snippet triggerSlot(triggerProps)}
    <button
      type="button"
      {...triggerProps}
      class="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      aria-label={$t("chat.sort.trigger")}
      title={$t("chat.sort.trigger")}
      data-testid="chat-sort-trigger"
    >
      <ArrowUpDown size={14} strokeWidth={1.75} aria-hidden="true" />
    </button>
  {/snippet}
  {#snippet body({ close })}
    <p class="px-2 pb-1 pt-1.5 text-caption text-muted-foreground">
      {$t("chat.sort.title")}
    </p>
    <ul class="flex flex-col gap-0.5" role="menu" data-testid="chat-sort-menu">
      {#each SESSION_SORTS as order (order)}
        <li role="none">
          <button
            type="button"
            role="menuitemradio"
            aria-checked={$sessionSort === order}
            class={cn(
              "flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-body-xs text-foreground transition-colors hover:bg-muted",
              $sessionSort === order && "bg-muted",
            )}
            onclick={() => {
              sessionSort.set(order);
              close();
            }}
            data-testid="chat-sort-{order}"
          >
            <span class="flex-1">{$t(SESSION_SORT_LABEL_KEYS[order])}</span>
            {#if $sessionSort === order}
              <Check size={14} aria-hidden="true" />
            {/if}
          </button>
        </li>
      {/each}
    </ul>
  {/snippet}
</ActionMenu>
