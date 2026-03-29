<script lang="ts">
  import { t } from "svelte-i18n";
  import {
    Database,
    Search,
    Folder,
    Brain,
    Globe,
    MessageSquare,
    Plug2,
  } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import type { RegistryServerView } from "$lib/types";
  import TrustBadge from "./TrustBadge.svelte";

  // All lucide-svelte icons share the same component shape — typed via typeof Database.
  const ICON_MAP: Record<string, typeof Database> = {
    database: Database,
    search: Search,
    folder: Folder,
    brain: Brain,
    globe: Globe,
    message: MessageSquare,
  };

  interface Props {
    server: RegistryServerView;
    onclick: () => void;
  }

  let { server, onclick }: Props = $props();

  const title = $derived(
    server.enrichment?.operator_label ?? server.title ?? server.name,
  );

  const Icon: typeof Database = $derived(
    server.enrichment ? (ICON_MAP[server.enrichment.icon_name] ?? Plug2) : Plug2,
  );

  const isDisabled = $derived(server.is_installed);
</script>

<button
  type="button"
  class="glass-card glass-border rounded-xl overflow-hidden flex flex-col text-left w-full transition-shadow duration-150
    {isDisabled
      ? 'opacity-70 cursor-default'
      : 'hover:shadow-md hover:border-primary/30 cursor-pointer'}"
  onclick={() => { if (!isDisabled) onclick(); }}
  disabled={isDisabled}
  data-testid="catalogue-card"
  data-server-name={server.name}
  aria-disabled={isDisabled}
>
  <div class="px-4 pt-3.5 pb-3 flex flex-col gap-2.5 flex-1">
    <!-- Row 1: icon + title + badges -->
    <div class="flex items-start gap-3">
      <div
        class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-muted text-muted-foreground"
        aria-hidden="true"
      >
        <Icon size={18} />
      </div>
      <div class="min-w-0 flex-1">
        <div class="flex items-center gap-2 flex-wrap">
          <span
            class="text-[13px] font-medium text-foreground truncate"
            data-testid="catalogue-card-title"
          >
            {title}
          </span>
          <TrustBadge level={server.trust_level} />
          {#if server.is_installed}
            <Badge variant="success" data-testid="catalogue-card-installed-badge">
              {$t("integrations.catalogue.installed_badge")}
            </Badge>
          {/if}
        </div>
      </div>
    </div>

    <!-- Row 2: description -->
    {#if server.description}
      <p
        class="text-xs text-muted-foreground line-clamp-2"
        data-testid="catalogue-card-description"
      >
        {server.description}
      </p>
    {/if}
  </div>
</button>
