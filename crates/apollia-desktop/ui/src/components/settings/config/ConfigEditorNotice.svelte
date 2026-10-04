<script lang="ts">
  /**
   * The notice at the head of the configuration editor: a restart awaited, the
   * file changed on disk (the one notice for it, with Compare and Reload), the
   * file reloaded, the file saved, or the runtime's refusal.
   */
  import { t } from "svelte-i18n";
  import { X } from "lucide-svelte";
  import { Banner } from "$lib/components/ui/banner";
  import { Button } from "$lib/components/ui/button";
  import type { EditorNotice } from "$lib/settings/configEditor";

  interface Props {
    notice: EditorNotice;
    onrestart: () => void;
    oncompare: () => void;
    onreload: () => void;
    ondismiss: () => void;
  }

  let { notice, onrestart, oncompare, onreload, ondismiss }: Props = $props();

  const VARIANT = {
    restart: "warning",
    "disk-changed": "warning",
    reloaded: "info",
    saved: "success",
    error: "destructive",
  } as const;
</script>

<Banner surface="card" variant={VARIANT[notice.kind]} data-testid="config-editor-notice-{notice.kind}">
  {#if notice.kind === "restart"}
    {$t("settings.config_editor.notice_restart")}
  {:else if notice.kind === "disk-changed"}
    {$t("settings.config_editor.notice_disk_changed")}
  {:else if notice.kind === "reloaded"}
    {$t("settings.config_editor.notice_reloaded")}
  {:else if notice.kind === "saved"}
    {$t("settings.config_editor.notice_saved")}
  {:else}
    {notice.message}
  {/if}

  {#snippet trailing()}
    {#if notice.kind === "restart"}
      <Button size="sm" variant="outline" onclick={onrestart} data-testid="config-editor-restart">
        {$t("settings.config_editor.restart_now")}
      </Button>
    {:else if notice.kind === "disk-changed"}
      <Button size="sm" variant="outline" onclick={oncompare} data-testid="config-editor-compare">
        {$t("settings.config_editor.compare")}
      </Button>
      <Button size="sm" variant="outline" onclick={onreload} data-testid="config-editor-reload">
        {$t("settings.config_editor.reload")}
      </Button>
    {:else}
      <Button
        size="icon-sm"
        variant="ghost"
        onclick={ondismiss}
        aria-label={$t("settings.config_editor.dismiss")}
        data-testid="config-editor-notice-dismiss"
      >
        <X size={14} />
      </Button>
    {/if}
  {/snippet}
</Banner>
