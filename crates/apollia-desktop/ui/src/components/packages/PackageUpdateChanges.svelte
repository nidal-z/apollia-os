<script lang="ts">
  /**
   * What an update adds, put forward in its preview: each new access and each
   * new connection with its own box, all to be ticked before updating. A new
   * connection this machine lacks also offers "Add".
   */
  import { t } from "svelte-i18n";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import type { ChangeToAccept } from "./packagePreviewState";

  interface Props {
    changes: ChangeToAccept[];
    accepted: ReadonlySet<string>;
    ontoggle: (key: string, checked: boolean) => void;
    onadd: (name: string) => void;
  }

  let { changes, accepted, ontoggle, onadd }: Props = $props();

  const accesses = $derived(changes.filter((c) => c.kind === "access"));
  const connections = $derived(changes.filter((c) => c.kind === "connection"));
</script>

<section
  class="flex flex-col gap-3 rounded-lg border border-warning/30 bg-warning/5 p-3"
  data-testid="package-preview-update-changes"
>
  {#if changes.length === 0}
    <p class="text-caption text-muted-foreground" data-testid="package-preview-update-nothing">
      {$t("packages.preview.update.nothing_new")}
    </p>
  {/if}
  {#if accesses.length > 0}
    <div class="flex flex-col gap-1.5">
      <h3 class="text-body-sm font-medium text-foreground">{$t("packages.preview.update.title_access")}</h3>
      {#each accesses as change (change.key)}
        <div class="flex items-start gap-2 text-caption text-foreground">
          <Checkbox
            checked={accepted.has(change.key)}
            onchange={(checked) => ontoggle(change.key, checked)}
            data-testid="package-preview-accept-{change.key}"
          />
          <span class="min-w-0 flex-1">
            {change.label}
            {#if change.family}
              <span class="text-muted-foreground">· {$t(`packages.preview.access.family.${change.family}.title`)}</span>
            {/if}
          </span>
          <Badge variant="warning">{$t("packages.preview.update.new_badge")}</Badge>
        </div>
      {/each}
    </div>
  {/if}
  {#if connections.length > 0}
    <div class="flex flex-col gap-1.5">
      <h3 class="text-body-sm font-medium text-foreground">{$t("packages.preview.update.title_connection")}</h3>
      {#each connections as change (change.key)}
        <div class="flex items-center gap-2 text-caption text-foreground">
          <Checkbox
            checked={accepted.has(change.key)}
            onchange={(checked) => ontoggle(change.key, checked)}
            data-testid="package-preview-accept-{change.key}"
          />
          <span class="min-w-0 flex-1">{change.label}</span>
          <Badge variant="warning">{$t("packages.preview.update.new_badge")}</Badge>
          {#if change.missing}
            <Button
              variant="outline"
              size="sm"
              onclick={() => onadd(change.label)}
              data-testid="package-preview-update-add-{change.label}"
            >
              {$t("packages.preview.connections.add")}
            </Button>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
  {#if changes.length > 0}
    <p class="text-caption text-muted-foreground">{$t("packages.preview.update.accept_hint")}</p>
  {/if}
  <p class="text-caption text-muted-foreground">{$t("packages.preview.update.overrides_kept")}</p>
</section>
