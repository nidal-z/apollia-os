<script lang="ts">
  /**
   * EngineRemoveDialog - the confirmation of the removal of one installed
   * engine build. The first confirmation sends the removal; when the build is
   * the one the GGUF choice names, the engine store keeps it and says what the
   * choice becomes, and the dialog says so before a second confirmation. A
   * build the store refuses to remove (in service, shipped with the
   * application, imposed by the policy) is said in words, with no way forward.
   */
  import { t } from "svelte-i18n";
  import { Loader2 } from "lucide-svelte";
  import Dialog from "$lib/components/ui/dialog/Dialog.svelte";
  import DialogFooter from "$lib/components/ui/dialog/DialogFooter.svelte";
  import { Button } from "$lib/components/ui/button";
  import { addToast } from "$lib/components/ui/toast/store";
  import { REMOVAL_REFUSAL_KEYS, type RemovalTarget } from "$lib/settings/engineManagement";
  import { engineManagement } from "$lib/settings/engineManagementLive";

  interface Props {
    /** The name of the build as its row writes it. */
    nameOf: (target: RemovalTarget) => string;
  }

  let { nameOf }: Props = $props();

  const removal = $derived($engineManagement.removal);
  const busy = $derived($engineManagement.busy);
  const name = $derived(removal ? nameOf(removal.target) : "");
  const tag = $derived(removal?.target.tag ?? "");

  const message = $derived.by(() => {
    const step = removal?.step;
    if (!step) return "";
    if (step.phase === "ask") return $t("settings.engines.remove.body");
    if (step.phase === "chosen") {
      return step.after === "auto"
        ? $t("settings.engines.remove.chosen_auto")
        : $t("settings.engines.remove.chosen_tag", { values: { tag: step.after } });
    }
    return $t(REMOVAL_REFUSAL_KEYS[step.code], { values: { reason: step.detail } });
  });

  async function confirm(): Promise<void> {
    const target = removal?.target;
    if (!target) return;
    const label = nameOf(target);
    if (await engineManagement.confirmRemove()) {
      addToast(
        $t("settings.engines.remove.done", { values: { name: label, tag: target.tag } }),
        "success",
        { "data-testid": "engine-variant-removed" },
      );
    }
  }
</script>

<Dialog
  open={removal !== null}
  onclose={() => engineManagement.cancelRemove()}
  size="sm"
  role="alertdialog"
  title={$t("settings.engines.remove.title", { values: { name, tag } })}
  data-testid="engine-variant-remove-dialog"
>
  {#if removal?.step.phase === "refused"}
    <p class="text-sm text-destructive" role="alert" data-testid="engine-variant-remove-dialog-error">
      {message}
    </p>
  {:else}
    <p
      class="text-sm {removal?.step.phase === 'chosen' ? 'text-warning' : 'text-muted-foreground'}"
      data-testid="engine-variant-remove-dialog-message"
    >
      {message}
    </p>
  {/if}

  <DialogFooter>
    <Button
      variant="outline"
      disabled={busy}
      onclick={() => engineManagement.cancelRemove()}
      data-testid="engine-variant-remove-dialog-cancel"
    >
      {removal?.step.phase === "refused"
        ? $t("settings.engines.remove.close")
        : $t("common.cancel")}
    </Button>
    {#if removal?.step.phase !== "refused"}
      <Button
        variant="destructive"
        disabled={busy}
        onclick={() => void confirm()}
        data-testid="engine-variant-remove-dialog-confirm"
      >
        {#if busy}<Loader2 class="mr-2 h-4 w-4 animate-spin" />{/if}
        {removal?.step.phase === "chosen"
          ? $t("settings.engines.remove.confirm_chosen")
          : $t("settings.engines.remove.confirm")}
      </Button>
    {/if}
  </DialogFooter>
</Dialog>
