<script lang="ts">
  import { t } from "svelte-i18n";
  import { ArrowDown } from "lucide-svelte";
  import { fly } from "svelte/transition";
  import { prefersReducedMotion } from "$lib/design/motion";

  interface Props {
    visible: boolean;
    unreadCount?: number;
    onclick: () => void;
  }

  let { visible, unreadCount = 0, onclick }: Props = $props();

  const transitionOpts = $derived(
    prefersReducedMotion() ? { duration: 120, y: 0 } : { duration: 180, y: 8 },
  );
</script>

{#if visible}
  <button
    type="button"
    {onclick}
    in:fly={transitionOpts}
    out:fly={transitionOpts}
    class="absolute bottom-4 left-1/2 z-10 -translate-x-1/2 inline-flex items-center gap-1.5 rounded-full border border-border/40 bg-card/95 px-3 py-1.5 text-caption font-medium text-foreground shadow-lg backdrop-blur hover:bg-card transition-colors"
    aria-label={$t("chat.scroll_to_bottom")}
    data-testid="chat-scroll-to-bottom"
  >
    <ArrowDown size={12} />
    <span>{$t("chat.scroll_to_bottom")}</span>
    {#if unreadCount > 0}
      <span
        class="inline-flex min-w-[18px] items-center justify-center rounded-full bg-primary px-1.5 text-micro font-semibold text-primary-foreground"
        data-testid="chat-scroll-unread-count"
      >
        {unreadCount > 99 ? "99+" : unreadCount}
      </span>
    {/if}
  </button>
{/if}
