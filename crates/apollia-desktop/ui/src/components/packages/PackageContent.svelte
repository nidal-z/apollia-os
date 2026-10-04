<script lang="ts">
  /**
   * What a package brings, in its preview: the count by kind, then one line
   * per agent, workflow and tool with what it does.
   */
  import { t } from "svelte-i18n";
  import { Bot, Workflow, Wrench } from "lucide-svelte";
  import type { PackagePreview } from "$lib/ipc/packageFiles";
  import { contentCounts, contentLines } from "./packagePreviewState";

  interface Props {
    preview: PackagePreview;
  }

  let { preview }: Props = $props();

  const lines = $derived(contentLines(preview));
  const counts = $derived(contentCounts(preview));
  const icons = { agent: Bot, workflow: Workflow, tool: Wrench } as const;
</script>

<section class="flex flex-col gap-2" data-testid="package-preview-content">
  <h3 class="text-caption font-medium uppercase tracking-wide text-muted-foreground">
    {$t("packages.preview.content.title")}
  </h3>
  <p class="text-caption text-muted-foreground" data-testid="package-preview-content-counts">
    {$t("packages.preview.content.counts", {
      values: { agents: counts.agent, workflows: counts.workflow, tools: counts.tool },
    })}
  </p>
  <ul class="flex flex-col gap-1.5">
    {#each lines as line (line.kind + ":" + line.name)}
      {@const Icon = icons[line.kind]}
      <li class="flex items-start gap-2 rounded-md border border-border bg-surface-1 px-2.5 py-2" data-testid="package-preview-content-{line.kind}">
        <Icon size={14} class="mt-0.5 shrink-0 text-muted-foreground" />
        <div class="min-w-0 flex-1">
          <p class="text-body-sm text-foreground">
            <span class="font-medium">{line.name}</span>
            <span class="text-caption text-muted-foreground">· {$t(`packages.preview.content.kind_${line.kind}`)}</span>
          </p>
          {#if line.summary}
            <p class="text-caption text-muted-foreground">{line.summary}</p>
          {/if}
        </div>
      </li>
    {/each}
  </ul>
  {#if preview.pip.length > 0}
    <p class="text-caption text-muted-foreground" data-testid="package-preview-python">
      {$t("packages.preview.content.python", { values: { count: preview.pip.length } })}
    </p>
  {/if}
</section>
