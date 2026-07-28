<script lang="ts">
  /**
   * OauthDriveFolders - per-account Google Drive root folder configuration.
   *
   * Self-contained: loads its own state from `oauth_list_drive_folders`. When no
   * Google account is connected the list is empty and nothing renders. Each
   * account exposes a tri-state root folder (default sub-folder / explicit Drive
   * root / custom path). Save and reset errors are humanized; success is toasted.
   *
   * The former Google Picker block (disabled behind `{#if false}`) has been
   * removed along with its plumbing; the backend commands remain for a future
   * re-enable.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { addToast } from "$lib/components/ui/toast";
  import { reportError } from "$lib/errors/reportError";
  import SettingsSection from "../settings/SettingsSection.svelte";
  import {
    listDriveFolders,
    setDriveFolder,
    resetDriveFolder,
    type DriveFolderStatus,
  } from "$lib/ipc/oauthClients";

  let folders = $state<DriveFolderStatus[]>([]);
  let drafts = $state<Record<string, string>>({});
  let savingAccount = $state<string | null>(null);

  async function refresh(): Promise<void> {
    try {
      const list = await listDriveFolders();
      folders = list;
      drafts = Object.fromEntries(
        list.map((f) => [f.account_id, f.folder_path ?? f.effective_folder_path]),
      );
    } catch {
      folders = [];
      drafts = {};
    }
  }

  async function save(accountId: string): Promise<void> {
    savingAccount = accountId;
    try {
      await setDriveFolder(accountId, (drafts[accountId] ?? "").trim());
      addToast($t("settings.integrations.drive.saved"), "success");
      await refresh();
    } catch (err) {
      reportError(err, { surface: "toast", testid: "drive-folder-error-toast" });
    } finally {
      savingAccount = null;
    }
  }

  async function reset(accountId: string): Promise<void> {
    savingAccount = accountId;
    try {
      await resetDriveFolder(accountId);
      addToast($t("settings.integrations.drive.reset_done"), "success");
      await refresh();
    } catch (err) {
      reportError(err, { surface: "toast", testid: "drive-folder-error-toast" });
    } finally {
      savingAccount = null;
    }
  }

  onMount(refresh);
</script>

{#if folders.length > 0}
  <SettingsSection
    title={$t("settings.integrations.drive.title")}
    description={$t("settings.integrations.drive.description")}
    data-testid="oauth-drive-folders-section"
  >
    <div class="space-y-3" data-testid="oauth-drive-folders">
    {#each folders as folder (folder.account_id)}
      <div class="space-y-2 rounded-md border border-border/60 bg-surface-1/50 px-3 py-2.5">
        <div class="font-mono text-caption text-foreground/85">{folder.account_id}</div>
        <div class="text-caption text-muted-foreground">
          {$t("settings.integrations.drive.effective")}
          {#if folder.folder_path === ""}
            <code class="rounded bg-muted px-1 py-px font-mono">
              {$t("settings.integrations.drive.drive_root")}
            </code>
            <span class="text-foreground/70">{$t("settings.integrations.drive.explicit")}</span>
          {:else}
            <code class="rounded bg-muted px-1 py-px font-mono">{folder.effective_folder_path}</code>
            {#if folder.folder_path === null}
              <span class="text-foreground/70">{$t("settings.integrations.drive.default_tag")}</span>
            {:else}
              <span class="text-foreground/70">{$t("settings.integrations.drive.custom_tag")}</span>
            {/if}
          {/if}
        </div>
        <div class="flex flex-wrap items-center gap-2">
          <Input
            type="text"
            bind:value={drafts[folder.account_id]}
            placeholder={$t("settings.integrations.drive.placeholder")}
            autocomplete="off"
            spellcheck={false}
            disabled={savingAccount === folder.account_id}
            class="min-w-0 flex-1 font-mono text-body-sm"
            data-testid={`drive-folder-input-${folder.account_id}`}
          />
          <Button
            variant="default"
            size="sm"
            onclick={() => save(folder.account_id)}
            loading={savingAccount === folder.account_id}
            disabled={savingAccount === folder.account_id}
            data-testid={`drive-folder-save-${folder.account_id}`}
          >
            {$t("common.save")}
          </Button>
          <Button
            variant="outline"
            size="sm"
            onclick={() => reset(folder.account_id)}
            disabled={savingAccount === folder.account_id || folder.folder_path === null}
            data-testid={`drive-folder-reset-${folder.account_id}`}
          >
            {$t("settings.integrations.drive.reset")}
          </Button>
        </div>
        <p class="max-w-prose text-caption text-muted-foreground">
          {$t("settings.integrations.drive.help")}
        </p>
      </div>
    {/each}
    </div>
  </SettingsSection>
{/if}
