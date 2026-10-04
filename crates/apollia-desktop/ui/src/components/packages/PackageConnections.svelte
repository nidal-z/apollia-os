<script lang="ts">
  /**
   * The connections of a package in its preview: those it requires, present
   * or missing, and those it brings. A missing one offers "Add", which opens
   * the add journey of the Connections screen.
   */
  import { t } from "svelte-i18n";
  import { Plug } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import type { PackagePreview } from "$lib/ipc/packageFiles";
  import { connectionLines } from "./packagePreviewState";

  interface Props {
    preview: PackagePreview;
    onadd: (name: string) => void;
  }

  let { preview, onadd }: Props = $props();

  const lines = $derived(connectionLines(preview));
  const variants = { missing: "warning", present: "success", brought: "info" } as const;
</script>

<section class="flex flex-col gap-2" data-testid="package-preview-connections">
  <h3 class="text-caption font-medium uppercase tracking-wide text-muted-foreground">
    {$t("packages.preview.connections.title")}
  </h3>
  {#if lines.length === 0}
    <p class="text-caption text-muted-foreground">{$t("packages.preview.connections.none")}</p>
  {:else}
    <ul class="flex flex-col gap-1.5">
      {#each lines as line (line.state + ":" + line.name)}
        <li
          class="flex items-center gap-2 rounded-md border border-border bg-surface-1 px-2.5 py-2"
          data-testid="package-preview-connection-{line.name}"
          data-state={line.state}
        >
          <Plug size={14} class="shrink-0 text-muted-foreground" />
          <div class="min-w-0 flex-1">
            <p class="text-body-sm font-medium text-foreground">{line.name}</p>
            {#if line.reason}
              <p class="text-caption text-muted-foreground">{line.reason}</p>
            {/if}
          </div>
          <Badge variant={variants[line.state]}>{$t(`packages.preview.connections.${line.state}`)}</Badge>
          {#if line.state === "missing"}
            <Button
              variant="outline"
              size="sm"
              onclick={() => onadd(line.name)}
              data-testid="package-preview-connection-add-{line.name}"
            >
              {$t("packages.preview.connections.add")}
            </Button>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</section>
