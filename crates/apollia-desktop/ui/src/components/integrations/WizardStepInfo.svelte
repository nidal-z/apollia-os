<script lang="ts">
  import { t } from "svelte-i18n";
  import { BookOpen, Globe } from "lucide-svelte";
  import TrustBadge from "./TrustBadge.svelte";
  import type { RegistryServerView } from "$lib/types";

  interface Props {
    server: RegistryServerView;
  }

  let { server }: Props = $props();
</script>

<div class="space-y-4" data-testid="wizard-step-info">
  <div class="flex items-start justify-between gap-3">
    <div class="min-w-0 flex-1">
      <h3 class="truncate text-base font-semibold text-foreground">
        {server.title ?? server.name}
      </h3>
      {#if server.description}
        <p class="mt-1 text-sm text-muted-foreground">{server.description}</p>
      {/if}
    </div>
    <TrustBadge level={server.trust_level} />
  </div>

  <div class="space-y-2 rounded-md border border-border bg-muted/40 p-4">
    <div class="flex items-center justify-between text-sm">
      <span class="text-muted-foreground">{$t("integrations.wizard.version")}</span>
      <span class="font-mono text-xs">{server.version}</span>
    </div>

    {#if server.repository_url}
      <div class="flex items-center justify-between text-sm">
        <span class="text-muted-foreground">{$t("integrations.wizard.repository")}</span>
        <a
          href={server.repository_url}
          target="_blank"
          rel="noopener noreferrer"
          class="flex items-center gap-1 text-xs text-primary underline-offset-4 hover:underline"
          data-testid="info-repo-link"
        >
          <BookOpen size={12} />
          {server.name}
        </a>
      </div>
    {/if}

    {#if server.website_url}
      <div class="flex items-center justify-between text-sm">
        <span class="text-muted-foreground">{$t("integrations.wizard.website")}</span>
        <a
          href={server.website_url}
          target="_blank"
          rel="noopener noreferrer"
          class="flex items-center gap-1 text-xs text-primary underline-offset-4 hover:underline"
          data-testid="info-website-link"
        >
          <Globe size={12} />
          {$t("integrations.wizard.visit_website")}
        </a>
      </div>
    {/if}
  </div>
</div>
