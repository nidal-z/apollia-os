<script lang="ts">
  /**
   * Warning banner displayed on macOS to inform users that tool execution
   * (bash_executor, file_io, etc.) runs without Linux namespace isolation.
   *
   * Shown once per session - dismissed state is stored in sessionStorage.
   * Renders nothing on non-macOS platforms.
   */
  import { t } from "svelte-i18n";
  import { AlertTriangle } from "lucide-svelte";
  import { Banner } from "$lib/components/ui/banner";

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
  <Banner variant="warning" surface="card" icon={AlertTriangle}>
    <span class="font-semibold text-warning">{$t("mac_sandbox.title")}</span>
    {" "}{$t("mac_sandbox.body")}

    {#snippet trailing()}
      <button
        class="shrink-0 text-warning/60 hover:text-warning transition-colors"
        onclick={dismiss}
        aria-label={$t("a11y.dismiss_warning")}
      >
        ✕
      </button>
    {/snippet}
  </Banner>
{/if}
