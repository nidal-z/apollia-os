<script lang="ts">
  /**
   * The organisation policy screen, read only. It loads the policy the
   * runtime applies and hands it to `PolicyScreen`; a failure to reach the
   * runtime is said as such, never as "no policy".
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { ErrorBanner } from "$lib/components/operator";
  import { getOrgPolicy, type PolicyView } from "$lib/ipc/policy";
  import PolicyScreen from "../components/policy/PolicyScreen.svelte";

  let view = $state<PolicyView | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);

  async function load(): Promise<void> {
    loading = true;
    error = null;
    try {
      view = await getOrgPolicy();
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void load();
  });
</script>

<div class="mx-auto flex w-full max-w-4xl flex-col gap-4 p-6" data-testid="policy-page">
  {#if loading && view === null}
    <Skeleton class="h-8 w-64" />
    <Skeleton class="h-40 w-full" />
  {:else if error}
    <ErrorBanner
      tone="danger"
      message={$t("policy.load_failed")}
      onretry={() => void load()}
      retryLabel={$t("common.retry")}
      {loading}
      data-testid="policy-load-error"
    />
  {:else if view}
    <PolicyScreen {view} />
  {/if}
</div>
