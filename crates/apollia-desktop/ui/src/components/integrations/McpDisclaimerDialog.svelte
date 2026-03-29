<script module lang="ts">
  export const DISCLAIMER_STORAGE_KEY = 'apollia-mcp-disclaimer-accepted';

  /// Returns true if the user has already accepted the MCP disclaimer.
  export function isDisclaimerAccepted(): boolean {
    return localStorage.getItem(DISCLAIMER_STORAGE_KEY) === 'true';
  }
</script>

<script lang="ts">
  import { t } from "svelte-i18n";
  import { ArrowUpRight, ShieldCheck, AlertTriangle, SlidersHorizontal } from "lucide-svelte";
  import { Dialog } from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import { Checkbox } from "$lib/components/ui/checkbox";

  interface Props {
    open: boolean;
    onaccept: () => void;
    onclose: () => void;
  }

  let { open, onaccept, onclose }: Props = $props();

  let checked = $state(false);

  $effect(() => {
    if (open) {
      checked = false;
    }
  });

  function handleAccept(): void {
    localStorage.setItem(DISCLAIMER_STORAGE_KEY, 'true');
    onaccept();
  }
</script>

<Dialog {open} {onclose} size="md" title={$t('integrations.disclaimer.title')}>
  <div class="space-y-4">
    <p class="text-sm text-muted-foreground">
      {$t('integrations.disclaimer.body_1')}
    </p>

    <div class="space-y-3 rounded-md border border-border bg-muted/40 p-4">
      <div class="flex gap-3">
        <ArrowUpRight size={16} class="mt-0.5 shrink-0 text-muted-foreground" />
        <div>
          <p class="text-sm font-medium">{$t('integrations.disclaimer.body_2_title')}</p>
          <p class="mt-0.5 text-xs text-muted-foreground">{$t('integrations.disclaimer.body_2')}</p>
        </div>
      </div>

      <div class="flex gap-3">
        <ShieldCheck size={16} class="mt-0.5 shrink-0 text-muted-foreground" />
        <div>
          <p class="text-sm font-medium">{$t('integrations.disclaimer.body_3_title')}</p>
          <p class="mt-0.5 text-xs text-muted-foreground">{$t('integrations.disclaimer.body_3')}</p>
        </div>
      </div>

      <div class="flex gap-3">
        <AlertTriangle size={16} class="mt-0.5 shrink-0 text-muted-foreground" />
        <div>
          <p class="text-sm font-medium">{$t('integrations.disclaimer.body_4_title')}</p>
          <p class="mt-0.5 text-xs text-muted-foreground">{$t('integrations.disclaimer.body_4')}</p>
        </div>
      </div>

      <div class="flex gap-3">
        <SlidersHorizontal size={16} class="mt-0.5 shrink-0 text-muted-foreground" />
        <div>
          <p class="text-sm font-medium">{$t('integrations.disclaimer.body_5_title')}</p>
          <p class="mt-0.5 text-xs text-muted-foreground">{$t('integrations.disclaimer.body_5')}</p>
        </div>
      </div>
    </div>

    <label class="flex cursor-pointer items-start gap-2" for="disclaimer-checkbox">
      <Checkbox
        id="disclaimer-checkbox"
        bind:checked
        data-testid="disclaimer-checkbox"
      />
      <span class="text-sm leading-tight">{$t('integrations.disclaimer.checkbox')}</span>
    </label>

    <div class="flex items-center justify-between pt-1">
      <button
        class="text-xs text-muted-foreground underline-offset-4 hover:underline"
        onclick={onclose}
        type="button"
      >
        {$t('integrations.disclaimer.learn_more')}
      </button>

      <div class="flex gap-2">
        <Button variant="outline" size="sm" onclick={onclose}>
          {$t('common.cancel')}
        </Button>
        <Button
          size="sm"
          disabled={!checked}
          onclick={handleAccept}
          data-testid="disclaimer-accept"
        >
          {$t('integrations.disclaimer.continue')}
        </Button>
      </div>
    </div>
  </div>
</Dialog>
