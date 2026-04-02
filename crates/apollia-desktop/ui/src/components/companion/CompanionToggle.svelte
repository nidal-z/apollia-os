<script lang="ts">
  import { companionStore, isCompanionToggleVisible } from "$lib/stores/companion";
  import { Bot } from "lucide-svelte";

  interface Props {
    /** Number of unread messages to show as a badge. */
    unreadCount?: number;
  }

  let { unreadCount = 0 }: Props = $props();

  const visible = $derived(isCompanionToggleVisible($companionStore));

  function handleClick() {
    companionStore.restoreCompanion();
  }
</script>

{#if visible}
  <button
    class="fixed z-[65] flex h-12 w-12 items-center justify-center rounded-full bg-primary text-primary-foreground shadow-lg transition-transform hover:scale-110 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    style:bottom="24px"
    style:right="24px"
    onclick={handleClick}
    aria-label="Ouvrir le Companion Apollia"
    data-testid="companion-toggle"
  >
    <Bot size={22} />
    {#if unreadCount > 0}
      <span
        class="absolute -right-1 -top-1 flex h-5 w-5 items-center justify-center rounded-full bg-destructive text-[10px] font-bold text-destructive-foreground"
        aria-label="{unreadCount} message(s) non lu(s)"
      >
        {unreadCount > 9 ? "9+" : unreadCount}
      </span>
    {/if}
  </button>
{/if}
