<script lang="ts">
  /**
   * GovernanceFailClosedBanner - the app-wide banner of a governance that
   * fails closed.
   *
   * Rendered under the top bar (see `Main.svelte`) while the permissions
   * database cannot be read or the organisation policy is refused. It follows
   * the `governanceFailClosed` store: an unreadable database is read again on
   * retry; a refused policy opens the permissions, where the policy is shown.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { addToast } from "$lib/components/ui/toast/store";
  import { openSettingsWindow } from "$lib/stores/settingsWindow";
  import {
    governanceFailClosed,
    refreshGovernanceFailClosed,
    startGovernanceFailClosedMonitor,
  } from "$lib/stores/governanceFailClosed";
  import FailClosedBannerView from "./FailClosedBannerView.svelte";

  let retrying = $state(false);

  onMount(() => startGovernanceFailClosedMonitor());

  async function retry(): Promise<void> {
    retrying = true;
    try {
      await refreshGovernanceFailClosed();
    } catch {
      addToast($t("governance.fail_closed.retry_failed"), "error");
    } finally {
      retrying = false;
    }
  }

  function viewPolicy(): void {
    openSettingsWindow("permissions");
  }
</script>

<FailClosedBannerView
  failClosed={$governanceFailClosed}
  {retrying}
  onretry={retry}
  onviewpolicy={viewPolicy}
/>
