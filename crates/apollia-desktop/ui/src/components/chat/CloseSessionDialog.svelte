<script lang="ts">
  /**
   * US-SP42-029 — Soft-close confirmation.
   *
   * "Close" an active session ends the conversation but keeps the transcript.
   * "Archive" a closed session hides it from the default filter. Distinct from
   * `delete` which destroys the session.
   */
  import { t } from "svelte-i18n";
  import Dialog from "$lib/components/ui/dialog/Dialog.svelte";
  import DialogFooter from "$lib/components/ui/dialog/DialogFooter.svelte";
  import { Button } from "$lib/components/ui/button";
  import { Loader2 } from "lucide-svelte";

  interface Props {
    open: boolean;
    mode: "close" | "archive";
    loading?: boolean;
    onclose: () => void;
    onconfirm: () => void;
  }

  let { open, mode, loading = false, onclose, onconfirm }: Props = $props();

  const title = $derived(
    mode === "close"
      ? $t("chat.close_dialog_title")
      : $t("chat.archive_dialog_title"),
  );
  const message = $derived(
    mode === "close"
      ? $t("chat.close_dialog_message")
      : $t("chat.archive_dialog_message"),
  );
  const confirmLabel = $derived(
    mode === "close" ? $t("chat.close_session") : $t("chat.archive_session"),
  );
</script>

<Dialog {open} {onclose} size="sm" {title} data-testid="chat-close-session-dialog">
  <p class="text-sm text-muted-foreground">{message}</p>

  <DialogFooter>
    <Button
      variant="outline"
      onclick={onclose}
      data-testid="chat-close-session-dialog-cancel"
    >
      {$t("common.cancel")}
    </Button>
    <Button
      variant={mode === "close" ? "default" : "outline"}
      onclick={onconfirm}
      disabled={loading}
      data-testid="chat-close-session-dialog-confirm"
    >
      {#if loading}
        <Loader2 class="mr-2 h-4 w-4 animate-spin" />
      {/if}
      {confirmLabel}
    </Button>
  </DialogFooter>
</Dialog>
