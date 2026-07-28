<script lang="ts">
  /**
   * ProfileDangerZone - the destructive reset action. Wipes every field and
   * memory fact through a confirm dialog, then relaunches onboarding.
   */
  import { t } from "svelte-i18n";
  import { AlertTriangle } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import { addToast } from "$lib/components/ui/toast";
  import { reportError } from "$lib/errors/reportError";
  import { resetUserProfile, triggerOnboarding } from "$lib/ipc/profile";

  interface Props {
    /** Called after a successful reset so the page can clear its state. */
    onReset: () => void;
  }

  let { onReset }: Props = $props();

  let open = $state(false);
  let resetting = $state(false);

  async function confirm(): Promise<void> {
    if (resetting) return;
    resetting = true;
    try {
      await resetUserProfile();
      onReset();
      addToast($t("settings.profile.toast.reset_done"), "success");
      open = false;
      await triggerOnboarding();
    } catch (err) {
      reportError(err, { surface: "toast" });
    } finally {
      resetting = false;
    }
  }
</script>

<div
  class="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-destructive/30 bg-destructive/5 p-4"
  data-testid="profile-danger"
>
  <div class="flex min-w-0 items-start gap-3">
    <AlertTriangle
      size={16}
      strokeWidth={1.75}
      class="mt-0.5 shrink-0 text-destructive"
      aria-hidden="true"
    />
    <div class="min-w-0">
      <h3 class="text-body-sm font-semibold text-destructive">
        {$t("settings.profile.danger.title")}
      </h3>
      <p class="mt-0.5 text-caption text-muted-foreground">
        {$t("settings.profile.danger.description")}
      </p>
    </div>
  </div>
  <Button
    variant="outline"
    size="sm"
    onclick={() => (open = true)}
    class="border-destructive/40 text-destructive hover:bg-destructive/10"
    data-testid="profile-reset-button"
  >
    {$t("settings.profile.danger.reset_button")}
  </Button>
</div>

<ConfirmDialog
  {open}
  onclose={() => (open = false)}
  onconfirm={confirm}
  title={$t("settings.profile.reset_dialog.title")}
  message={$t("settings.profile.reset_dialog.message")}
  confirmLabel={$t("settings.profile.reset_dialog.confirm")}
  cancelLabel={$t("common.cancel")}
  loading={resetting}
  data-testid="profile-reset-confirm"
/>
