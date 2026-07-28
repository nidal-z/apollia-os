<script lang="ts">
  /**
   * CredentialErrorNotice - a humanized credential error line with the raw
   * backend text tucked behind a Disclosure. Shared by CredentialField (tool
   * credentials) and the OAuth credential rows so both present save/test
   * failures identically: friendly guidance up front, raw detail on demand.
   */
  import { t } from "svelte-i18n";
  import { XCircle } from "lucide-svelte";
  import { Disclosure } from "$lib/components/ui/disclosure";
  import type { HumanizedError } from "$lib/errors/humanize";

  interface Props {
    error: HumanizedError;
    "data-testid"?: string;
  }

  let { error, "data-testid": testid }: Props = $props();
</script>

<div class="space-y-1.5" data-testid={testid}>
  <p class="flex items-start gap-1.5 text-caption text-danger-a11y">
    <XCircle size={13} strokeWidth={2} class="mt-px shrink-0" aria-hidden="true" />
    <span>
      <span class="font-medium">{error.friendly_message}</span>
      {error.suggested_action}
    </span>
  </p>
  {#if error.detail}
    <Disclosure>
      {#snippet summary()}{$t("settings.integrations.raw_detail")}{/snippet}
      <p class="break-all font-mono text-caption text-destructive/90">{error.detail}</p>
    </Disclosure>
  {/if}
</div>
