<script lang="ts">
  /**
   * CatalogueEntryRow - a single row in the discover catalogue.
   *
   * A canonical clickable `ListRow` styled as a bordered card with the
   * expressive hover lift. Trust badge, vendor, description, and an install /
   * manage CTA. Scales to long vendor names without truncation nightmares.
   */
  import { t } from "svelte-i18n";
  import { Plus, Link as LinkIcon } from "lucide-svelte";
  import { ListRow } from "$lib/components/operator";
  import { Badge } from "$lib/components/ui/badge";
  import ConnectorTile from "../shared/ConnectorTile.svelte";
  import { accentTokenFor } from "$lib/connections/accent";
  import { vendorLabel } from "$lib/connections/vendor";
  import type { RegistryServerView } from "$lib/types";

  interface Props {
    entry: RegistryServerView;
    installed: boolean;
    official: boolean;
    onactivate: () => void;
  }

  let { entry, installed, official, onactivate }: Props = $props();

  const displayName = $derived(entry.enrichment?.operator_label ?? entry.title ?? entry.name);
  const description = $derived(entry.description ?? entry.enrichment?.category ?? "");
  const vendor = $derived(vendorLabel(entry));
</script>

<ListRow
  variant="row"
  border={false}
  pad="snug"
  class="rounded-lg border border-border/60 bg-card elev-interactive hover:border-primary/40"
  onclick={onactivate}
  data-testid="catalogue-entry-{entry.name}"
>
  <ConnectorTile size="md" accent={accentTokenFor(displayName)}>
    {#snippet icon()}<LinkIcon size={14} />{/snippet}
  </ConnectorTile>
  <div class="min-w-0 flex-1">
    <div class="mb-0.5 flex min-w-0 flex-wrap items-center gap-2">
      <span class="truncate text-body-sm font-medium text-foreground">{displayName}</span>
      {#if official}
        <Badge size="sm" variant="primary">{$t("connections.badge_official")}</Badge>
      {:else}
        <Badge size="sm" variant="neutral" outline>{$t("connections.badge_community")}</Badge>
      {/if}
      {#if installed}
        <Badge size="sm" variant="success">{$t("connections.badge_installed")}</Badge>
      {/if}
    </div>
    {#if vendor}
      <div class="mb-0.5 text-caption text-muted-foreground">
        {$t("connections.catalogue_by")}
        <span class="font-medium text-foreground/80">{vendor}</span>
      </div>
    {/if}
    {#if description}
      <p class="line-clamp-2 text-caption text-muted-foreground">{description}</p>
    {/if}
  </div>
  <div class="shrink-0 self-center">
    {#if installed}
      <span class="text-caption font-medium text-primary">{$t("connections.catalogue_manage")}</span>
    {:else}
      <span class="inline-flex items-center gap-1 text-caption font-medium text-primary">
        <Plus size={11} />
        {$t("connections.connect")}
      </span>
    {/if}
  </div>
</ListRow>
