<script lang="ts">
  /**
   * GovernanceFailClosedBanner - the app-wide banner of a governance that
   * fails closed.
   *
   * Rendered under the top bar (see `Main.svelte`) while the permissions
   * database cannot be read or the organisation policy is refused. It follows
   * the `governanceFailClosed` store: an unreadable database is read again on
   * retry; a refused policy opens the authorizations screen, whose policy
   * panel says why and leads to the policy itself.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { addToast } from "$lib/components/ui/toast/store";
  import { navigateTo } from "$lib/stores/navigation";
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
    navigateTo("authorizations");
  }
</script>

<FailClosedBannerView
  failClosed={$governanceFailClosed}
  {retrying}
  onretry={retry}
  onviewpolicy={viewPolicy}
/>
