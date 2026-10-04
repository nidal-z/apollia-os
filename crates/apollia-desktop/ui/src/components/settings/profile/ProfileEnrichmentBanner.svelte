<script lang="ts">
  /**
   * ProfileEnrichmentBanner - the agent's part in a profile, in two states.
   *
   * Invitation: once name and role are set but a field is still empty, offers
   * a short conversation with the onboarding agent for the displayed profile.
   * The agent writes nothing: its proposal comes back to this page.
   *
   * Review: the proposal is laid over the form below, unsaved. "Save" writes
   * the form, "Skip" puts back what the profile holds. Nothing is written
   * before "Save".
   */
  import { t } from "svelte-i18n";
  import { fly } from "svelte/transition";
  import { Button } from "$lib/components/ui/button";
  import { rowIn } from "$lib/design/listMotion";
  import { onboardingModalOpen, profileEnrichment } from "$lib/stores/onboarding";
  import type { ProfileFieldView } from "$lib/ipc/profile";
  import { shouldOfferEnrichment } from "./profileForm";

  interface Props {
    profileId: string;
    fields: ProfileFieldView[];
    /** A proposal of the agent is in the form, waiting to be saved or skipped. */
    reviewing: boolean;
    onsave: () => void;
    onskip: () => void;
  }

  let { profileId, fields, reviewing, onsave, onskip }: Props = $props();

  let dismissed = $state(false);

  const invite = $derived(!reviewing && !dismissed && shouldOfferEnrichment(fields));

  function resume(): void {
    profileEnrichment.set({ profileId, fields });
    onboardingModalOpen.set(true);
  }
</script>

{#if reviewing}
  <div
    class="flex flex-col gap-3 rounded-xl border border-primary bg-primary/5 p-4 sm:flex-row sm:items-center sm:justify-between"
    transition:fly={rowIn()}
    data-testid="profile-proposal-banner"
  >
    <div class="flex min-w-0 flex-col gap-0.5">
      <span class="text-body-sm font-semibold text-foreground">
        {$t("settings.profile.proposal.title")}
      </span>
      <span class="text-caption text-muted-foreground">
        {$t("settings.profile.proposal.detail")}
      </span>
    </div>
    <div class="flex shrink-0 items-center gap-2">
      <Button variant="ghost" size="sm" onclick={onskip} data-testid="profile-proposal-skip">
        {$t("settings.profile.proposal.skip")}
      </Button>
      <Button
        variant="primary-gradient"
        size="sm"
        onclick={onsave}
        data-testid="profile-proposal-save"
      >
        {$t("settings.profile.proposal.save")}
      </Button>
    </div>
  </div>
{:else if invite}
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
        variant="ghost"
        size="sm"
        onclick={() => (dismissed = true)}
        data-testid="profile-enrichment-dismiss"
      >
        {$t("settings.profile.enrichment.dismiss")}
      </Button>
      <Button
        variant="primary-gradient"
        size="sm"
        onclick={resume}
        data-testid="profile-enrichment-resume"
      >
        {$t("settings.profile.enrichment.resume")}
      </Button>
    </div>
  </div>
{/if}
