<script module lang="ts">
  /** A save into a preset that waits for its confirmation. */
  export interface PendingPresetSave {
    /** The preset written, `null` while it has no name yet. */
    name: string | null;
    /** The i18n keys of the settings changed, in the order they are shown. */
    fieldKeys: string[];
    /** The save itself: nothing is written until it is called. */
    run: () => void | Promise<void>;
  }

  /**
   * What becomes of a save into a preset. With the confirmation off, it runs
   * at once and nothing is held. With it on, it is held, not run: the dialog
   * runs it on "Save" and drops it on "Cancel".
   */
  export function holdPresetSave(
    confirm: boolean,
    pending: PendingPresetSave,
  ): PendingPresetSave | null {
    if (confirm) return pending;
    void pending.run();
    return null;
  }

  /** Runs a held save; the caller forgets it. */
  export function confirmPresetSave(pending: PendingPresetSave | null): void {
    if (pending) void pending.run();
  }
</script>

<script lang="ts">
  /**
   * The confirmation asked before settings are saved into a preset, when
   * Settings > General turns it on: the settings changed, then "Save" or
   * "Cancel". "Cancel" writes nothing.
   */
  import { t } from "svelte-i18n";
  import { Dialog, DialogFooter } from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    /** The save held, `null` when there is none: the dialog is then closed. */
    pending: PendingPresetSave | null;
    /** Called once the held save was run or dropped. */
    onsettled: () => void;
    testid: string;
  }

  let { pending, onsettled, testid }: Props = $props();

  function confirm(): void {
    const held = pending;
    onsettled();
    confirmPresetSave(held);
  }
</script>

<Dialog
  open={pending !== null}
  onclose={onsettled}
  size="sm"
  title={$t("presets.save_confirm.title")}
  data-testid={testid}
>
  {#if pending}
    <p class="text-sm text-muted-foreground" data-testid="{testid}-message">
      {#if pending.fieldKeys.length === 0}
        {$t("presets.save_confirm.no_fields")}
      {:else if pending.name !== null}
        {$t("presets.save_confirm.message_named", { values: { name: pending.name } })}
      {:else}
        {$t("presets.save_confirm.message")}
      {/if}
    </p>
    {#if pending.fieldKeys.length > 0}
      <ul class="mt-3 list-disc space-y-1 pl-5 text-sm text-foreground" data-testid="{testid}-fields">
        {#each pending.fieldKeys as key (key)}
          <li data-testid="{testid}-field">{$t(key)}</li>
        {/each}
      </ul>
    {/if}
  {/if}
  <DialogFooter>
    <Button variant="ghost" onclick={onsettled} data-testid="{testid}-cancel">
      {$t("common.cancel")}
    </Button>
    <Button variant="primary-solid" onclick={confirm} data-testid="{testid}-confirm">
      {$t("presets.save_confirm.confirm")}
    </Button>
  </DialogFooter>
</Dialog>
