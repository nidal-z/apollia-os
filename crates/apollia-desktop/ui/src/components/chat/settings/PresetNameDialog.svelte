<script lang="ts">
  /**
   * Asks the name of a preset about to be created. `onsubmit` answers the
   * reason of a refusal, shown under the field, or `null` once created.
   */
  import { t } from "svelte-i18n";
  import { Dialog, DialogFooter } from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { FormField } from "$lib/components/ui/form-field";

  interface Props {
    open: boolean;
    title: string;
    /** Name the field starts from. */
    initial?: string;
    onclose: () => void;
    onsubmit: (name: string) => Promise<string | null>;
    testid: string;
  }

  let { open, title, initial = "", onclose, onsubmit, testid }: Props = $props();

  let name = $state("");
  let busy = $state(false);
  let refusal = $state<string | null>(null);

  $effect(() => {
    if (open) {
      name = initial;
      refusal = null;
    }
  });

  async function submit(): Promise<void> {
    if (name.trim() === "" || busy) return;
    busy = true;
    try {
      refusal = await onsubmit(name.trim());
    } finally {
      busy = false;
    }
  }
</script>

<Dialog {open} {onclose} size="sm" {title} data-testid={testid}>
  <FormField
    id="{testid}-name"
    label={$t("chat.session_settings.preset.name_label")}
    error={refusal ?? undefined}
  >
    <Input
      id="{testid}-name"
      bind:value={name}
      maxlength={80}
      onkeydown={(e: KeyboardEvent) => {
        if (e.key === "Enter") void submit();
      }}
      data-testid="{testid}-name"
    />
  </FormField>
  <DialogFooter>
    <Button variant="ghost" onclick={onclose} data-testid="{testid}-cancel">
      {$t("common.cancel")}
    </Button>
    <Button
      variant="primary-solid"
      onclick={() => void submit()}
      disabled={name.trim() === ""}
      loading={busy}
      data-testid="{testid}-confirm"
    >
      {$t("chat.session_settings.preset.create")}
    </Button>
  </DialogFooter>
</Dialog>
