<script lang="ts" module>
  /**
   * Whether the confirmation can go through: always when nothing has to be
   * typed, otherwise once what was typed is the expected text, to the letter
   * but for the spaces around it.
   */
  export function typedConfirmationMatches(expected: string | undefined, typed: string): boolean {
    return expected === undefined || typed.trim() === expected.trim();
  }
</script>

<script lang="ts">
  import Dialog from "./Dialog.svelte";
  import { FormField } from "$lib/components/ui/form-field";
  import { Input } from "$lib/components/ui/input";
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
    /**
     * Text the person has to type before the confirmation goes through, for a
     * removal that cannot be taken back: the name of what is removed.
     */
    requireText?: string;
    /** Label of the field `requireText` is typed in. Required with it. */
    requireTextLabel?: string;
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
    requireText,
    requireTextLabel = "",
    "data-testid": dataTestId,
  }: Props = $props();

  let typed = $state("");

  const fieldId = $derived(`${dataTestId ?? "confirm-dialog"}-typed`);
  const confirmable = $derived(typedConfirmationMatches(requireText, typed));

  $effect(() => {
    // What was typed for one confirmation never carries over to the next.
    if (!open) typed = "";
  });
</script>

<Dialog {open} {onclose} size="sm" {title} data-testid={dataTestId}>
  <p class="text-sm text-muted-foreground">{message}</p>

  {#if requireText !== undefined}
    <FormField id={fieldId} label={requireTextLabel} class="mt-4">
      <Input
        id={fieldId}
        type="text"
        bind:value={typed}
        autocomplete="off"
        spellcheck={false}
        data-testid={dataTestId ? `${dataTestId}-typed-name` : undefined}
      />
    </FormField>
  {/if}

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
      disabled={loading || !confirmable}
      data-testid={dataTestId ? `${dataTestId}-confirm` : undefined}
    >
      {#if loading}
        <Loader2 class="mr-2 h-4 w-4 animate-spin" />
      {/if}
      {confirmLabel ?? $t("common.confirm")}
    </Button>
  </DialogFooter>
</Dialog>
