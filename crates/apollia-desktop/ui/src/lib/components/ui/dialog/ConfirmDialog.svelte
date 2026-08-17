<script lang="ts">
  import Dialog from "./Dialog.svelte";
  import DialogFooter from "./DialogFooter.svelte";
  import { Button } from "$lib/components/ui/button";
  import { Loader2 } from "lucide-svelte";
  import { t } from "svelte-i18n";

  interface Props {
    open: boolean;
    onclose: () => void;
    onconfirm: () => void;
    title: string;
    message: string;
    /** Confirm button label. Falls back to `common.confirm`. */
    confirmLabel?: string;
    /** Cancel button label. Falls back to `common.cancel`. */
    cancelLabel?: string;
    loading?: boolean;
    "data-testid"?: string;
  }

  let {
    open,
    onclose,
    onconfirm,
    title,
    message,
    confirmLabel,
    cancelLabel,
    loading = false,
    "data-testid": dataTestId,
  }: Props = $props();
</script>

<Dialog {open} {onclose} size="sm" {title} data-testid={dataTestId}>
  <p class="text-sm text-muted-foreground">{message}</p>

  <DialogFooter>
    <Button
      variant="outline"
      onclick={onclose}
      data-testid={dataTestId ? `${dataTestId}-cancel` : undefined}
    >
      {cancelLabel ?? $t("common.cancel")}
    </Button>
    <Button
      variant="destructive"
      onclick={onconfirm}
      disabled={loading}
      data-testid={dataTestId ? `${dataTestId}-confirm` : undefined}
    >
      {#if loading}
        <Loader2 class="mr-2 h-4 w-4 animate-spin" />
      {/if}
      {confirmLabel ?? $t("common.confirm")}
    </Button>
  </DialogFooter>
</Dialog>
