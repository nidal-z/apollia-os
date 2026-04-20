<script lang="ts">
  /**
   * HTTP/network call preview (US-SP42-054). Shows method, URL, headers,
   * and an estimate of the request body size.
   */
  import { t } from "svelte-i18n";
  import { Globe } from "lucide-svelte";
  import type { NetworkPreview } from "./types";

  interface Props {
    payload: NetworkPreview;
  }

  let { payload }: Props = $props();

  function formatBytes(n: number | undefined): string {
    if (!n) return "—";
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    return `${(n / 1024 / 1024).toFixed(2)} MB`;
  }
</script>

<div class="space-y-3" data-testid="preview-network">
  <header class="flex items-center gap-2 text-xs">
    <Globe size={14} class="text-muted-foreground" />
    <span class="rounded bg-primary/10 px-2 py-0.5 font-mono font-semibold uppercase text-primary">
      {payload.method}
    </span>
    <code class="min-w-0 flex-1 truncate font-mono text-[12px]">{payload.url}</code>
  </header>

  {#if payload.headers?.length}
    <section>
      <h4 class="mb-1 text-[11px] uppercase tracking-wide text-muted-foreground">
        {$t("permissions.preview.network.headers")}
      </h4>
      <table class="w-full text-[12px]">
        <tbody>
          {#each payload.headers as h, i (i)}
            <tr class="border-t border-border">
              <td class="py-1 pr-3 font-mono text-muted-foreground">{h.name}</td>
              <td class="py-1 font-mono">{h.value}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </section>
  {/if}

  <section class="flex items-center gap-3 text-[12px]">
    <span class="text-muted-foreground">{$t("permissions.preview.network.body_size")}</span>
    <span class="font-mono">{formatBytes(payload.estimated_body_bytes)}</span>
  </section>

  {#if payload.body_excerpt}
    <section>
      <h4 class="mb-1 text-[11px] uppercase tracking-wide text-muted-foreground">
        {$t("permissions.preview.network.body_excerpt")}
      </h4>
      <pre class="max-h-[200px] overflow-auto rounded bg-muted px-3 py-2 text-[11px]">{payload.body_excerpt}</pre>
    </section>
  {/if}
</div>
