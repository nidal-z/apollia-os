<script lang="ts">
  /**
   * PackageOverviewTab - install metadata for a package: install date, source
   * path, and author (when the loaded detail is available).
   */
  import { t } from "svelte-i18n";
  import { Clock, FolderOpen, Users } from "lucide-svelte";
  import { Card } from "$lib/components/operator";
  import type { AgentPackageDetailView, AgentPackageListItem } from "$lib/types";

  interface Props {
    pkg: AgentPackageListItem;
    detail: AgentPackageDetailView | null;
  }

  let { pkg, detail }: Props = $props();
</script>

<Card class="max-w-3xl p-3.5">
  <div class="mb-2 text-body-xs font-semibold text-foreground">
    {$t("agents.info_title")}
  </div>
  <div class="flex flex-col gap-2 text-caption text-muted-foreground">
    <div class="flex items-center gap-1.5">
      <Clock size={11} />
      <span>
        {$t("agents.installed_on", {
          values: { date: new Date(pkg.installed_at).toLocaleDateString() },
        })}
      </span>
    </div>
    <div class="flex items-center gap-1.5">
      <FolderOpen size={11} class="shrink-0" />
      <span class="truncate font-mono text-caption" title={pkg.root_path}>
        {pkg.root_path}
      </span>
    </div>
    {#if detail?.author}
      <div class="flex items-center gap-1.5">
        <Users size={11} />
        <span>{detail.author}</span>
      </div>
    {/if}
  </div>
</Card>
