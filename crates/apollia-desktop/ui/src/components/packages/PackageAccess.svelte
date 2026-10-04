<script lang="ts">
  /**
   * The accesses the agents of a package ask for, grouped by action family,
   * each family with what it lets an agent do and each access with what it
   * permits here.
   */
  import { t } from "svelte-i18n";
  import { Badge } from "$lib/components/ui/badge";
  import type { PackagePreview } from "$lib/ipc/packageFiles";
  import { accessGroups } from "./packagePreviewState";

  interface Props {
    preview: PackagePreview;
  }

  let { preview }: Props = $props();

  const groups = $derived(accessGroups(preview));
</script>

<section class="flex flex-col gap-2" data-testid="package-preview-access">
  <h3 class="text-caption font-medium uppercase tracking-wide text-muted-foreground">
    {$t("packages.preview.access.title")}
  </h3>
  {#if groups.length === 0}
    <p class="text-caption text-muted-foreground" data-testid="package-preview-access-none">
      {$t("packages.preview.access.none")}
    </p>
  {:else}
    <ul class="flex flex-col gap-2">
      {#each groups as group (group.family)}
        <li class="rounded-md border border-border bg-surface-1 px-2.5 py-2" data-testid="package-preview-family-{group.family}">
          <p class="text-body-sm font-medium text-foreground">
            {$t(`packages.preview.access.family.${group.family}.title`)}
          </p>
          <p class="text-caption text-muted-foreground">
            {$t(`packages.preview.access.family.${group.family}.hint`)}
          </p>
          <ul class="mt-1.5 flex flex-col gap-1">
            {#each group.accesses as access (access.accessId)}
              <li class="flex items-start gap-2 text-caption text-foreground">
                <span class="min-w-0 flex-1">{access.permits}</span>
                {#if access.sensitive}
                  <Badge variant="warning">{$t("packages.preview.access.sensitive")}</Badge>
                {/if}
              </li>
            {/each}
          </ul>
        </li>
      {/each}
    </ul>
  {/if}
</section>
