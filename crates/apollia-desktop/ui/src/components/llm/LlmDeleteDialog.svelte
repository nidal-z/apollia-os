<script lang="ts">
  /**
   * LlmDeleteDialog - confirmation dialog for removing a backend.
   *
   * Deleting the default backend requires typing DELETE, mirroring the settings
   * CRUD safeguard. Runs the IPC itself and reports success via `ondeleted`.
   */
  import { t } from "svelte-i18n";
  import type { LlmBackendConfig } from "$lib/types";
  import { deleteLlmBackend } from "$lib/ipc/llm";
  import { reportError } from "$lib/errors/reportError";
  import { addToast } from "$lib/components/ui/toast";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import Dialog from "$lib/components/ui/dialog/Dialog.svelte";
  import DialogFooter from "$lib/components/ui/dialog/DialogFooter.svelte";

  interface Props {
    target: LlmBackendConfig | null;
    onclose: () => void;
    ondeleted: () => void;
  }

  let { target, onclose, ondeleted }: Props = $props();

  let confirmText = $state("");
  let deleting = $state(false);

  const requiresType = $derived(!!target?.is_default);
  const canConfirm = $derived(!!target && (!requiresType || confirmText === "DELETE"));

  $effect(() => {
    // Reset the typed confirmation whenever a new target is opened.
    void target;
    confirmText = "";
  });

  async function confirm() {
    if (!target || !canConfirm) return;
    deleting = true;
    try {
      await deleteLlmBackend(target.name);
      addToast($t("settings.llm.delete_toast", { values: { name: target.name } }), "success");
      ondeleted();
    } catch (err: unknown) {
      reportError(err, { surface: "toast" });
    } finally {
      deleting = false;
    }
  }
</script>

<Dialog
  open={!!target}
  {onclose}
  size="sm"
  title={$t("settings.llm.delete_title")}
  data-testid="llm-delete-dialog"
>
  {#if target}
    <p class="text-sm text-muted-foreground">
      {$t("settings.llm.delete_message", { values: { name: target.name } })}
    </p>
    {#if requiresType}
      <div class="mt-4 space-y-1.5">
        <label for="llm-delete-confirm" class="text-xs font-medium text-foreground">
          {$t("settings.llm.delete_type_prompt")}
        </label>
        <Input
          id="llm-delete-confirm"
          type="text"
          placeholder="DELETE"
          class="h-9 w-full font-mono"
          bind:value={confirmText}
          data-testid="llm-delete-confirm-input"
        />
      </div>
    {/if}
  {/if}
  <DialogFooter>
    <Button variant="outline" onclick={onclose} data-testid="llm-delete-cancel">
      {$t("common.cancel")}
    </Button>
    <Button
      variant="destructive"
      onclick={confirm}
      disabled={deleting || !canConfirm}
      data-testid="llm-delete-confirm-btn"
    >
      {deleting ? $t("common.loading") : $t("common.delete")}
    </Button>
  </DialogFooter>
</Dialog>
