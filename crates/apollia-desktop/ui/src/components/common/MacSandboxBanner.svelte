<script lang="ts">
  /**
   * Warning banner displayed on macOS to inform users that tool execution
   * (bash_executor, file_io, etc.) runs without Linux namespace isolation.
   *
   * Shown once per session — dismissed state is stored in sessionStorage.
   * Renders nothing on non-macOS platforms.
   */
  import { t } from "svelte-i18n";

  const STORAGE_KEY = "apollia_mac_sandbox_dismissed";

  const isMacOS =
    typeof navigator !== "undefined" &&
    (navigator.platform.startsWith("Mac") ||
      navigator.userAgent.includes("Mac OS X"));

  let dismissed = $state(
    typeof sessionStorage !== "undefined"
      ? sessionStorage.getItem(STORAGE_KEY) === "1"
      : true
  );

  function dismiss() {
    dismissed = true;
    try {
      sessionStorage.setItem(STORAGE_KEY, "1");
    } catch {
      // sessionStorage unavailable
    }
  }
</script>

{#if isMacOS && !dismissed}
  <div
    class="flex items-start gap-3 rounded-lg border border-amber-500/40 bg-amber-500/8 px-4 py-3 text-sm"
    role="alert"
  >
    <span class="mt-0.5 shrink-0 text-base leading-none text-amber-400">⚠</span>
    <div class="flex-1 text-amber-200/90">
      <span class="font-semibold text-amber-300">{$t('mac_sandbox.title')}</span>
      {" "}{$t('mac_sandbox.body')}
    </div>
    <button
      class="shrink-0 text-amber-400/60 hover:text-amber-300 transition-colors"
      onclick={dismiss}
      aria-label="Dismiss warning"
    >
      ✕
    </button>
  </div>
{/if}
