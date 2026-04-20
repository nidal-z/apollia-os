<script lang="ts">
  import { t } from "svelte-i18n";
  import { Bot } from "lucide-svelte";
  import { companionStore } from "$lib/stores/companion";
  import { sidebarState } from "$lib/stores/layout";

  interface Props {
    unreadCount?: number;
    pulse?: boolean;
  }

  let { unreadCount = 0, pulse = false }: Props = $props();

  // Position: bottom-right above dock when sidebar collapsed, else
  // bottom-left above the sidebar rail.
  const alignRight = $derived($sidebarState !== "expanded");

  function handleClick() {
    companionStore.restoreCompanion();
  }
</script>

<button
  type="button"
  class="group fixed z-[65] flex h-12 w-12 items-center justify-center rounded-full bg-primary text-primary-foreground shadow-lg outline-none transition-transform hover:scale-110 focus-visible:ring-2 focus-visible:ring-ring"
  class:companion-pulse={pulse}
  style:bottom="24px"
  style:right={alignRight ? "24px" : undefined}
  style:left={alignRight ? undefined : "24px"}
  onclick={handleClick}
  aria-label={$t("companion.restore")}
  title={$t("companion.restore")}
  data-testid="companion-restore"
>
  <Bot size={22} />
  {#if unreadCount > 0}
    <span
      class="absolute -right-1 -top-1 flex h-5 min-w-5 items-center justify-center rounded-full bg-destructive px-1 text-[10px] font-semibold text-destructive-foreground"
      data-testid="companion-restore-badge"
      aria-label={$t("companion.restore_unread", {
        values: { count: unreadCount },
      })}
    >
      {unreadCount > 9 ? "9+" : unreadCount}
    </span>
  {/if}
</button>

<style>
  @keyframes companion-pulse {
    0%,
    100% {
      opacity: 1;
      box-shadow: 0 0 0 0 rgba(var(--primary-rgb, 59, 130, 246), 0.35);
    }
    50% {
      opacity: 0.8;
      box-shadow: 0 0 0 8px rgba(var(--primary-rgb, 59, 130, 246), 0);
    }
  }
  .companion-pulse {
    animation: companion-pulse 2s ease-in-out infinite;
  }
</style>
