<script lang="ts">
  /**
   * ProfileResetDialog - confirms the reset of one profile. Any profile can be
   * reset alone; the default one also offers to get acquainted again through
   * the onboarding conversation (`resetChoices`).
   */
  import { t } from "svelte-i18n";
  import { Button } from "$lib/components/ui/button";
  import Dialog from "$lib/components/ui/dialog/Dialog.svelte";
  import DialogFooter from "$lib/components/ui/dialog/DialogFooter.svelte";
  import type { ProfileSummaryView } from "$lib/ipc/profile";
  import { resetChoices, type ResetChoice } from "./profileForm";

  interface Props {
    /** The profile to reset; `null` keeps the dialog closed. */
    profile: ProfileSummaryView | null;
    busy: boolean;
    onclose: () => void;
    onconfirm: (choice: ResetChoice) => void;
  }

  let { profile, busy, onclose, onconfirm }: Props = $props();

  const choices = $derived(profile ? resetChoices(profile) : []);
  const offersOnboarding = $derived(choices.includes("reset_and_onboard"));
</script>

<Dialog
  open={profile !== null}
  {onclose}
  size="md"
  role="alertdialog"
  title={$t(
    offersOnboarding
      ? "settings.profile.reset_dialog.title_default"
      : "settings.profile.reset_dialog.title",
    { values: { name: profile?.name ?? "" } },
  )}
  data-testid="profile-reset-dialog"
>
  <p class="text-sm text-muted-foreground">
    {$t(
      offersOnboarding
        ? "settings.profile.reset_dialog.message_default"
        : "settings.profile.reset_dialog.message",
    )}
  </p>
  <DialogFooter>
    <Button variant="ghost" onclick={onclose} data-testid="profile-reset-cancel">
      {$t("common.cancel")}
    </Button>
    {#if offersOnboarding}
      <Button
        variant="outline"
        loading={busy}
        onclick={() => onconfirm("reset")}
        data-testid="profile-reset-only"
      >
        {$t("settings.profile.reset_dialog.reset_only")}
      </Button>
      <Button
        variant="destructive"
        loading={busy}
        onclick={() => onconfirm("reset_and_onboard")}
        data-testid="profile-reset-and-onboard"
      >
        {$t("settings.profile.reset_dialog.reset_and_onboard")}
      </Button>
    {:else}
      <Button
        variant="destructive"
        loading={busy}
        onclick={() => onconfirm("reset")}
        data-testid="profile-reset-confirm"
      >
        {$t("settings.profile.reset_dialog.confirm")}
      </Button>
    {/if}
  </DialogFooter>
</Dialog>
