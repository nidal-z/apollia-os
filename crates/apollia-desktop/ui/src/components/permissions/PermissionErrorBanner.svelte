<script lang="ts">
  /**
   * Humanized banner for backend permission errors (US-SP42-054, E.36).
   *
   * Uses the offline pattern mapper; when the error doesn't match any
   * known pattern, the raw message is shown inside a generic
   * "Unexpected error" envelope — callers can opt into a Meta-LLM
   * fallback upstream by passing `humanizedOverride`.
   */
  import { t } from "svelte-i18n";
  import { AlertCircle, ExternalLink } from "lucide-svelte";
  import { permissionErrorHumanize, type HumanizedError } from "$lib/permissions/errorHumanize";

  interface Props {
    error: string | null | undefined;
    /** Override returned by the Meta-LLM fallback. */
    humanizedOverride?: HumanizedError;
  }

  let { error, humanizedOverride }: Props = $props();

  const humanized = $derived.by<HumanizedError>(() => {
    if (humanizedOverride) return humanizedOverride;
    const mapped = permissionErrorHumanize(error);
    if (mapped) return mapped;
    return {
      title: $t("permissions.error.unknown_title"),
      friendly_message: error ?? $t("permissions.error.unknown_body"),
      suggested_action: $t("permissions.error.unknown_action"),
    };
  });
</script>

<div
  class="flex items-start gap-2 rounded-md border border-destructive/40 bg-destructive/5 px-3 py-2 text-xs"
  role="alert"
  data-testid="permission-error-banner"
  data-error-code={humanized.code ?? "UNKNOWN"}
>
  <AlertCircle size={14} class="mt-0.5 shrink-0 text-destructive" />
  <div class="min-w-0 flex-1">
    <p class="font-semibold text-destructive">{humanized.title}</p>
    <p class="mt-0.5 text-muted-foreground">{humanized.friendly_message}</p>
    <p class="mt-1">
      <span class="font-medium">{$t("permissions.error.suggested_action")}</span>
      <span class="ml-1 text-muted-foreground">{humanized.suggested_action}</span>
    </p>
    {#if humanized.learn_more_url}
      <a
        href={humanized.learn_more_url}
        class="mt-1 inline-flex items-center gap-1 text-primary hover:underline"
        target="_blank"
        rel="noopener noreferrer"
      >
        {$t("permissions.error.learn_more")}
        <ExternalLink size={11} />
      </a>
    {/if}
  </div>
</div>
