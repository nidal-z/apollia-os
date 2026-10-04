<script lang="ts">
  /**
   * A line-by-line comparison of two texts of `apollia.toml`, in a dialog:
   * before an explicit action that replaces the text being edited (back to the
   * default values, the file found on disk), never before a plain save.
   */
  import { t } from "svelte-i18n";
  import Dialog from "$lib/components/ui/dialog/Dialog.svelte";
  import DialogFooter from "$lib/components/ui/dialog/DialogFooter.svelte";
  import { Button } from "$lib/components/ui/button";
  import { hasChanges, type DiffLine } from "$lib/settings/configText";

  interface Props {
    open: boolean;
    title: string;
    message: string;
    diff: DiffLine[];
    confirmLabel: string;
    /** What the dialog says when the two texts are the same. */
    unchangedLabel: string;
    onconfirm: () => void;
    onclose: () => void;
    busy?: boolean;
    "data-testid": string;
  }

  let {
    open,
    title,
    message,
    diff,
    confirmLabel,
    unchangedLabel,
    onconfirm,
    onclose,
    busy = false,
    "data-testid": testid,
  }: Props = $props();

  const changed = $derived(hasChanges(diff));

  const ROW: Record<DiffLine["kind"], string> = {
    same: "text-muted-foreground",
    added: "bg-success/10 text-foreground",
    removed: "bg-destructive/10 text-foreground",
  };
  const SIGN: Record<DiffLine["kind"], string> = { same: " ", added: "+", removed: "-" };
</script>

<Dialog {open} {onclose} size="lg" {title} data-testid={testid}>
  <p class="text-body-sm text-muted-foreground">{message}</p>

  {#if changed}
    <div
      class="mt-3 max-h-[50vh] overflow-auto rounded-lg border border-border bg-surface-1 py-2 font-mono text-code-sm leading-6"
      data-testid="{testid}-diff"
    >
      {#each diff as line, index (index)}
        <div
          class="flex whitespace-pre px-3 {ROW[line.kind]}"
          data-kind={line.kind}
          title={line.kind === "added"
            ? $t("settings.config_editor.diff_added")
            : line.kind === "removed"
              ? $t("settings.config_editor.diff_removed")
              : undefined}
        >
          <span class="w-4 flex-none select-none" aria-hidden="true">{SIGN[line.kind]}</span>
          <span>{line.text}</span>
        </div>
      {/each}
    </div>
  {:else}
    <p class="mt-3 text-body-sm text-foreground" data-testid="{testid}-unchanged">
      {unchangedLabel}
    </p>
  {/if}

  <DialogFooter>
    <Button variant="outline" onclick={onclose} data-testid="{testid}-cancel">
      {$t("common.cancel")}
    </Button>
    <Button onclick={onconfirm} disabled={busy} data-testid="{testid}-confirm">
      {confirmLabel}
    </Button>
  </DialogFooter>
</Dialog>
