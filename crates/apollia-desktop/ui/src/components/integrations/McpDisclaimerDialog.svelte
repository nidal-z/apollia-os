<script module lang="ts">
  // Legacy one-checkbox storage kept for backwards compatibility with prior
  // wizard versions. New code reads `DISCLAIMER_VERSION_KEY` from
  // `WizardStepDisclaimer` to detect whether the current disclaimer version
  // has been accepted.
  export const DISCLAIMER_STORAGE_KEY = "apollia-mcp-disclaimer-accepted";

  /// Returns true if the user has ever accepted any version of the disclaimer.
  export function isDisclaimerAccepted(): boolean {
    return localStorage.getItem(DISCLAIMER_STORAGE_KEY) === "true";
  }
</script>

<script lang="ts">
  import { t } from "svelte-i18n";
  import { Dialog } from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import WizardStepDisclaimer, {
    DISCLAIMER_ITEMS,
    recordDisclaimerAcceptance,
  } from "./WizardStepDisclaimer.svelte";

  interface Props {
    open: boolean;
    onaccept: () => void;
    onclose: () => void;
  }

  let { open, onaccept, onclose }: Props = $props();

  let checks = $state<Record<string, boolean>>({});

  $effect(() => {
    if (open) {
      checks = {};
    }
  });

  const allChecked = $derived(
    DISCLAIMER_ITEMS.every((key) => {
      const short = key.replace("i18n:integrations.disclaimer.items.", "");
      return checks[short] === true;
    }),
  );

  function handleChange(key: string, value: boolean): void {
    checks = { ...checks, [key]: value };
  }

  async function handleAccept(): Promise<void> {
    await recordDisclaimerAcceptance();
    localStorage.setItem(DISCLAIMER_STORAGE_KEY, "true");
    onaccept();
  }
</script>

<Dialog {open} {onclose} size="md" title={$t("integrations.disclaimer.title")}>
  <div class="space-y-4">
    <WizardStepDisclaimer {checks} onchange={handleChange} />

    <div class="flex items-center justify-end gap-2 pt-1">
      <Button variant="outline" size="sm" onclick={onclose}>
        {$t("common.cancel")}
      </Button>
      <Button
        size="sm"
        disabled={!allChecked}
        onclick={() => void handleAccept()}
        data-testid="disclaimer-accept"
      >
        {$t("integrations.disclaimer.continue")}
      </Button>
    </div>
  </div>
</Dialog>
