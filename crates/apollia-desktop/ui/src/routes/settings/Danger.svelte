<script lang="ts" context="module">
  export const meta = {
    title: "settings.nav.danger",
    icon: "alert-triangle",
    group: "settings.nav.cluster_danger",
    cluster: "danger",
  } as const;
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { Button } from "$lib/components/ui/button";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import { navigateTo } from "$lib/stores/navigation";
  import { onboardingStore } from "$lib/stores/onboarding";

  let resettingOnboarding = $state(false);
  let showResetConfirm = $state(false);
  let error = $state<string | null>(null);

  async function confirmResetOnboarding() {
    resettingOnboarding = true;
    error = null;
    try {
      await invoke("reset_onboarding");
      onboardingStore.setRequired();
      navigateTo("onboarding");
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      resettingOnboarding = false;
      showResetConfirm = false;
    }
  }
</script>

<section class="space-y-4" data-testid="danger-section">
  <p class="text-sm text-muted-foreground">{$t('settings.danger.subtitle')}</p>

  {#if error}
    <div class="rounded-md border border-destructive/30 bg-destructive/5 px-4 py-2 text-sm text-destructive">{error}</div>
  {/if}

  <div class="rounded-lg border border-destructive/30 bg-destructive/5 p-4">
    <div class="flex items-start justify-between gap-4">
      <div>
        <p class="text-sm font-medium text-foreground">{$t('settings.review_onboarding')}</p>
        <p class="text-xs text-muted-foreground">{$t('settings.review_onboarding_desc')}</p>
      </div>
      <Button
        variant="outline"
        size="sm"
        onclick={() => { showResetConfirm = true; }}
        data-testid="reset-onboarding-btn"
      >
        {$t('settings.reset_onboarding')}
      </Button>
    </div>
  </div>
</section>

<ConfirmDialog
  open={showResetConfirm}
  onclose={() => { showResetConfirm = false; }}
  onconfirm={confirmResetOnboarding}
  title={$t('settings.reset_confirm_title')}
  message={$t('settings.reset_confirm_message')}
  confirmLabel={$t('settings.reset_onboarding')}
  cancelLabel={$t('common.cancel')}
  loading={resettingOnboarding}
  data-testid="reset-onboarding-confirm"
/>
