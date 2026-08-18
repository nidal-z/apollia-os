<script lang="ts">
  /**
   * PermissionErrorBanner - the showcase surface for `lib/errors/humanize`.
   *
   * The Permissions page is the primary consumer of the humanizer: every raw
   * backend / IPC error is turned into title + friendly message + suggested
   * action, with the coarse category `code` for telemetry, an optional
   * "Learn more" deep link, and the raw text tucked behind a Details
   * disclosure. It never shows a naked `Error: EACCES errno -13`.
   */
  import { t } from "svelte-i18n";
  import { Sparkles, ExternalLink } from "lucide-svelte";
  import { ErrorBanner } from "$lib/components/operator";
  import { Disclosure } from "$lib/components/ui/disclosure";
  import { humanize } from "$lib/errors/humanize";
  import { handleExternalLinkClick } from "$lib/utils/externalLink";

  interface Props {
    /** Raw error (string / Error / unknown) to humanize. */
    error: unknown;
    tone?: "danger" | "warning" | "info";
    onretry?: () => void;
    loading?: boolean;
    "data-testid"?: string;
  }

  let {
    error,
    tone = "danger",
    onretry,
    loading = false,
    "data-testid": testid,
  }: Props = $props();

  const humanized = $derived(humanize(error, $t));
</script>

<ErrorBanner
  {tone}
  {onretry}
  {loading}
  retryLabel={$t("common.retry")}
  data-testid={testid}
>
  <div class="flex flex-col gap-1.5">
    <p class="text-label-md text-foreground">{humanized.title}</p>
    <p class="text-caption opacity-90">{humanized.friendly_message}</p>
    <p class="flex items-start gap-1.5 text-caption">
      <Sparkles size={13} class="mt-px shrink-0 text-primary" aria-hidden="true" />
      <span>{humanized.suggested_action}</span>
    </p>

    {#if humanized.detail || humanized.code || humanized.learn_more_url}
      <div class="mt-1 flex flex-wrap items-center gap-2.5">
        {#if humanized.code}
          <code
            class="rounded bg-muted px-1.5 py-px font-mono text-caption text-muted-foreground"
            data-testid="permissions-error-code"
          >
            {humanized.code}
          </code>
        {/if}
        {#if humanized.learn_more_url}
          <a
            href={humanized.learn_more_url}
            target="_blank"
            rel="noreferrer"
            onclick={handleExternalLinkClick}
            class="inline-flex items-center gap-1 text-caption text-primary underline-offset-2 outline-none hover:underline focus-visible:ring-2 focus-visible:ring-ring/60"
          >
            <ExternalLink size={12} aria-hidden="true" />
            {$t("settings.permissions.error_learn_more")}
          </a>
        {/if}
      </div>

      {#if humanized.detail}
        <div class="mt-1.5">
          <Disclosure testid="permissions-error-details">
            {#snippet summary()}
              <span class="text-caption">{$t("settings.permissions.error_details")}</span>
            {/snippet}
            <pre
              class="whitespace-pre-wrap break-words font-mono text-caption text-muted-foreground">{humanized.detail}</pre>
          </Disclosure>
        </div>
      {/if}
    {/if}
  </div>
</ErrorBanner>
