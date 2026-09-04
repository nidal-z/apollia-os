<script lang="ts">
  import { t, locale } from "svelte-i18n";
  import { AlertCircle, ExternalLink, RotateCw, ScrollText } from "lucide-svelte";
  import { Dialog } from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import type { McpServerStatusView, ConnectorEnrichmentView } from "$lib/types";
  import { handleExternalLinkClick } from "$lib/utils/externalLink";
  import { docsUrlFor } from "$lib/utils/docsUrl";

  interface Props {
    open: boolean;
    server: McpServerStatusView | null;
    enrichment: ConnectorEnrichmentView | null;
    onclose: () => void;
    onretry: (name: string) => void;
    onviewLogs: (name: string) => void;
  }

  let { open, server, enrichment, onclose, onretry, onviewLogs }: Props = $props();

  // The fallback used to build its own URL on a host that answers NXDOMAIN,
  // under a route the site does not publish. It now lands on the page that does
  // exist and covers exactly this moment, resolved under the interface locale.
  // The host itself is named nowhere here: docsUrlSites.test.ts refuses it
  // outside the builder, and prose reads the same as a literal to a scan.
  const docsUrl = $derived(
    enrichment?.auth_help_url ??
      docsUrlFor($locale, "/operator-help/integrations/connect-an-mcp-server"),
  );

  const title = $derived(enrichment?.operator_label ?? server?.name ?? "");
</script>

<Dialog {open} {onclose} size="md" title={$t("connections.error.title")} data-testid="connection-error-modal">
  {#if server}
    <div class="flex flex-col gap-4">
      <div class="flex items-start gap-3">
        <div class="shrink-0 rounded-full bg-destructive/10 p-2 text-destructive">
          <AlertCircle size={18} />
        </div>
        <div class="min-w-0 flex-1">
          <p class="text-sm font-medium text-foreground">{title}</p>
          <p class="mt-1 text-xs text-muted-foreground break-words" data-testid="connection-error-detail">
            {server.error ?? $t("connections.error.unknown")}
          </p>
        </div>
      </div>

      <div class="flex flex-wrap items-center justify-end gap-2 pt-2">
        {#if docsUrl}
          <a
            href={docsUrl}
            target="_blank"
            rel="noopener noreferrer"
            onclick={handleExternalLinkClick}
            class="inline-flex items-center gap-1.5 text-xs text-muted-foreground hover:text-foreground"
            data-testid="connection-error-docs"
          >
            <ExternalLink size={13} />
            {$t("connections.error.consult_docs")}
          </a>
        {/if}
        <Button
          size="sm"
          variant="outline"
          onclick={() => onviewLogs(server.name)}
          data-testid="connection-error-view-logs"
        >
          <ScrollText size={14} class="mr-1.5" />
          {$t("connections.error.view_logs")}
        </Button>
        <Button size="sm" onclick={() => onretry(server.name)} data-testid="connection-error-retry">
          <RotateCw size={14} class="mr-1.5" />
          {$t("connections.error.retry")}
        </Button>
      </div>
    </div>
  {/if}
</Dialog>
