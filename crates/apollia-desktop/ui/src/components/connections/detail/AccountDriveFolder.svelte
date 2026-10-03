<script lang="ts">
  /**
   * AccountDriveFolder - the Drive folder a connected Google account's agents
   * work in, under the account on its connection's page: shown, changed, or
   * brought back to the default.
   */
  import { t } from "svelte-i18n";
  import { Folder } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { addToast } from "$lib/components/ui/toast";
  import { reportError } from "$lib/errors/reportError";
  import { resetDriveFolder, saveDriveFolder } from "$lib/connections/oauthClient";
  import type { DriveFolderStatus } from "$lib/ipc/oauthClients";

  interface Props {
    accountId: string;
    folder: DriveFolderStatus | undefined;
    /** The folder changed: the parent reads the folders again. */
    onchanged: () => void;
  }

  let { accountId, folder, onchanged }: Props = $props();

  let editing = $state(false);
  let draft = $state("");
  let saving = $state(false);

  const shown = $derived(
    folder?.folder_path === ""
      ? $t("settings.integrations.drive.drive_root")
      : (folder?.effective_folder_path ?? ""),
  );

  function startEdit(): void {
    draft = folder?.folder_path ?? folder?.effective_folder_path ?? "";
    editing = true;
  }

  async function save(): Promise<void> {
    saving = true;
    try {
      await saveDriveFolder(accountId, draft);
      addToast($t("settings.integrations.drive.saved"), "success");
      editing = false;
      onchanged();
    } catch (err) {
      reportError(err, { surface: "toast", testid: "drive-folder-error-toast" });
    } finally {
      saving = false;
    }
  }

  async function reset(): Promise<void> {
    saving = true;
    try {
      await resetDriveFolder(accountId);
      addToast($t("settings.integrations.drive.reset_done"), "success");
      onchanged();
    } catch (err) {
      reportError(err, { surface: "toast", testid: "drive-folder-error-toast" });
    } finally {
      saving = false;
    }
  }
</script>

<div class="mt-2 flex flex-wrap items-center gap-2 text-caption text-muted-foreground" data-testid="account-drive-folder-{accountId}">
  <Folder size={12} aria-hidden="true" />
  {#if editing}
    <Input
      type="text"
      bind:value={draft}
      placeholder={$t("settings.integrations.drive.placeholder")}
      aria-label={$t("settings.integrations.drive.folder_path")}
      autocomplete="off"
      spellcheck={false}
      disabled={saving}
      class="min-w-0 flex-1 font-mono text-body-sm"
      data-testid="account-drive-folder-input-{accountId}"
    />
    <Button
      variant="ghost"
      size="sm"
      onclick={() => (editing = false)}
      disabled={saving}
      data-testid="account-drive-folder-cancel-{accountId}"
    >
      {$t("common.cancel")}
    </Button>
    <Button
      variant="primary-solid"
      size="sm"
      onclick={save}
      loading={saving}
      disabled={saving}
      data-testid="account-drive-folder-save-{accountId}"
    >
      {$t("common.save")}
    </Button>
  {:else}
    <span class="min-w-0 flex-1 truncate">
      {$t("connections.oauth_client.drive_folder", { values: { path: shown } })}
      {#if folder?.folder_path === null}
        {$t("settings.integrations.drive.default_tag")}
      {/if}
    </span>
    <Button
      variant="ghost"
      size="sm"
      onclick={startEdit}
      disabled={saving}
      data-testid="account-drive-folder-edit-{accountId}"
    >
      {$t("connections.oauth_client.drive_folder_edit")}
    </Button>
    <Button
      variant="ghost"
      size="sm"
      onclick={reset}
      disabled={saving || folder?.folder_path === null || folder === undefined}
      data-testid="account-drive-folder-reset-{accountId}"
    >
      {$t("connections.oauth_client.drive_folder_reset")}
    </Button>
  {/if}
</div>
