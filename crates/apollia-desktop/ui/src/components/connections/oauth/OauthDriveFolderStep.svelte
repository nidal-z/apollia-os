<script lang="ts">
  /**
   * OauthDriveFolderStep - Google post-auth step: pick the Drive root path.
   *
   * With the free-tier `drive.file` scope Apollia cannot list existing folders,
   * so the user types the path and Apollia creates each missing segment.
   */
  import { t } from "svelte-i18n";
  import { Input } from "$lib/components/ui/input";

  interface Props {
    draft: string;
    saving: boolean;
    error: string | null;
  }

  let { draft = $bindable(), saving, error }: Props = $props();
</script>

<div class="space-y-3" data-testid="oauth-dialog-drive-folder">
  <p class="text-body-sm text-muted-foreground">{@html $t("connections.oauth_drive_folder_body_html")}</p>
  <p class="text-caption text-muted-foreground">{@html $t("connections.oauth_drive_folder_expert_html")}</p>
  <label class="block text-label-sm font-medium text-foreground">
    {@html $t("connections.oauth_drive_folder_path_label_html")}
    <Input
      type="text"
      class="mt-1 font-mono"
      placeholder="Apollia"
      bind:value={draft}
      disabled={saving}
      autocomplete="off"
      data-testid="oauth-drive-folder-input"
    />
  </label>
  <p class="text-caption text-muted-foreground">{@html $t("connections.oauth_drive_folder_examples_html")}</p>
  {#if error}
    <p class="text-caption text-destructive" data-testid="oauth-drive-folder-error">{error}</p>
  {/if}
</div>
