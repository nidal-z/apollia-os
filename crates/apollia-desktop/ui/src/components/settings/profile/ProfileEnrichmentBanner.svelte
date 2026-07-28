<script lang="ts">
  /**
   * ProfileEnrichmentBanner - the "complete your profile" nudge. Once
   * calibration (name + role) is done but optional Tier 2 fields are still
   * empty, offers to resume the onboarding agent (which preserves collected
   * answers) rather than filling the form by hand.
   */
  import { t } from "svelte-i18n";
  import { fly } from "svelte/transition";
  import { Button } from "$lib/components/ui/button";
  import { rowIn } from "$lib/design/listMotion";
  import { onboardingModalOpen, onboardingResumeMode } from "$lib/stores/onboarding";
  import { TIER2_KEYS, type ProfileFieldApi } from "./profileForm";

  interface Props {
    api: ProfileFieldApi;
  }

  let { api }: Props = $props();

  let dismissed = $state(false);

  const calibrationDone = $derived(
    !!api.values["name"]?.trim() && !!api.values["role"]?.trim(),
  );
  const missingCount = $derived(
    TIER2_KEYS.filter((k) => !api.values[k]?.trim()).length,
  );
  const show = $derived(!dismissed && calibrationDone && missingCount > 0);

  function resume(): void {
    onboardingResumeMode.set(true);
    onboardingModalOpen.set(true);
  }
</script>

{#if show}
  <div
    class="flex flex-col gap-3 rounded-xl border border-primary/30 bg-primary/5 p-4 sm:flex-row sm:items-center sm:justify-between"
    transition:fly={rowIn()}
    data-testid="profile-enrichment-banner"
  >
    <div class="flex min-w-0 flex-col gap-0.5">
      <span class="text-body-sm font-semibold text-foreground">
        {$t("settings.profile.enrichment.title")}
      </span>
      <span class="text-caption text-muted-foreground">
        {$t("settings.profile.enrichment.detail")}
      </span>
    </div>
    <div class="flex shrink-0 items-center gap-2">
      <Button
        variant="primary-gradient"
        size="sm"
        onclick={resume}
        data-testid="profile-enrichment-resume"
      >
        {$t("settings.profile.enrichment.resume")}
      </Button>
      <Button
        variant="ghost"
        size="sm"
        onclick={() => (dismissed = true)}
        data-testid="profile-enrichment-dismiss"
      >
        {$t("settings.profile.enrichment.dismiss")}
      </Button>
    </div>
  </div>
{/if}
