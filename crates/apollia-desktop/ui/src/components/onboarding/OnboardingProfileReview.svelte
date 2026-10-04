<script lang="ts">
  /**
   * The proposal of the onboarding conversation, read over before anything is
   * written.
   *
   * Shows the profile the agent proposed in the form of Settings > Profile
   * ({@link ProfileFieldGrid}), over the default profile. No field is
   * required. "Save" writes the reviewed values to the default profile;
   * "Skip without saving" writes nothing. Both end the onboarding.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { Button } from "$lib/components/ui/button";
  import { ErrorBanner } from "$lib/components/operator";
  import { reportError } from "$lib/errors/reportError";
  import type { HumanizedError } from "$lib/errors/humanize";
  import {
    DEFAULT_PROFILE,
    getProfileById,
    updateProfile,
    type ProfileFieldView,
  } from "$lib/ipc/profile";
  import {
    applyReview,
    reviewValues,
    type ProfileProposal,
    type ReviewChoice,
  } from "$lib/onboarding/profileProposal";
  import ProfileFieldGrid from "../settings/profile/ProfileFieldGrid.svelte";

  interface Props {
    /** What the agent proposed; `null` when the person skipped the questions. */
    proposal: ProfileProposal | null;
    /** Called once the review ended, saved or not. */
    ondone: () => void;
    /** Back to the conversation; absent when there is none to go back to. */
    onback?: () => void;
  }

  const { proposal, ondone, onback }: Props = $props();

  let fields = $state<ProfileFieldView[]>([]);
  let values = $state<Record<string, string>>({});
  let loading = $state(true);
  let busy = $state(false);
  // Inline and not a toast: a toast is painted under the onboarding veil.
  let failure = $state<HumanizedError | null>(null);

  onMount(async () => {
    try {
      const detail = await getProfileById(DEFAULT_PROFILE);
      fields = detail.fields;
      values = reviewValues(detail.fields, proposal);
    } catch (err) {
      failure = reportError(err, { surface: "inline" });
    } finally {
      loading = false;
    }
  });

  async function end(choice: ReviewChoice): Promise<void> {
    if (busy) return;
    busy = true;
    failure = null;
    try {
      await applyReview(choice, DEFAULT_PROFILE, fields, values, { update: updateProfile });
      ondone();
    } catch (err) {
      // The form stays open with what the person typed: a failed save is not
      // a reason to lose it, and they can still leave without saving.
      failure = reportError(err, { surface: "inline" });
    } finally {
      busy = false;
    }
  }
</script>

<div class="flex min-h-0 flex-1 flex-col" data-testid="onboarding-profile-review">
  <div class="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-6 py-5">
    <div class="flex flex-col gap-1">
      <h3 class="text-heading-md font-semibold text-foreground">
        {$t("onboarding_review.title")}
      </h3>
      <p class="text-body-sm text-muted-foreground">
        {$t("onboarding_review.description")}
      </p>
    </div>

    {#if failure}
      <ErrorBanner data-testid="onboarding-review-error">
        <p class="font-medium">{failure.friendly_message}</p>
        <p class="text-caption opacity-90">{failure.suggested_action}</p>
      </ErrorBanner>
    {/if}

    {#if loading}
      <p class="text-body-sm text-muted-foreground" data-testid="onboarding-review-loading">
        {$t("onboarding_review.loading")}
      </p>
    {:else}
      <ProfileFieldGrid
        {fields}
        {values}
        disabled={busy}
        onchange={(key: string, value: string) => (values[key] = value)}
      />
    {/if}
  </div>

  <div class="flex shrink-0 items-center justify-between gap-3 border-t border-border/60 px-6 py-3">
    {#if onback}
      <Button
        variant="ghost"
        size="sm"
        onclick={onback}
        disabled={busy}
        data-testid="onboarding-review-back"
      >
        {$t("onboarding_review.back")}
      </Button>
    {:else}
      <span></span>
    {/if}
    <div class="flex items-center gap-2">
      <Button
        variant="outline"
        size="sm"
        onclick={() => end("skip")}
        disabled={busy}
        data-testid="onboarding-review-skip"
      >
        {$t("onboarding_review.skip")}
      </Button>
      <Button
        variant="default"
        size="sm"
        onclick={() => end("save")}
        disabled={busy || loading || fields.length === 0}
        data-testid="onboarding-review-save"
      >
        {$t("onboarding_review.save")}
      </Button>
    </div>
  </div>
</div>
