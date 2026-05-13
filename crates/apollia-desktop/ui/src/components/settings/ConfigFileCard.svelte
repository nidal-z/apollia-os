<script lang="ts">
  import { t } from "svelte-i18n";
  import { ChevronDown, ChevronRight, Copy, ExternalLink, CheckCircle2, AlertCircle, FileX } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { addToast } from "$lib/components/ui/toast";
  import { navigateTo } from "$lib/stores/navigation";
  import type { ConfigSection } from "$lib/types";
  import { Card } from "$lib/components/ui/card";

  interface Props {
    section: ConfigSection;
    filePath?: string;
    fileExists: boolean;
    defaultExpanded?: boolean;
  }

  let { section, filePath, fileExists, defaultExpanded = false }: Props = $props();

  let expanded = $state(defaultExpanded);
  let rawOpen = $state(false);

  const status = $derived(fileExists ? "exists" : "missing");

  async function copyPath() {
    if (!filePath) return;
    try {
      await navigator.clipboard.writeText(filePath);
      addToast($t("settings.config.path_copied"), "success");
    } catch {
      addToast($t("settings.config.copy_failed"), "error");
    }
  }

  function gotoRedirect() {
    if (section.redirect_route) {
      navigateTo(section.redirect_route as Parameters<typeof navigateTo>[0]);
    }
  }

  const rawValue = $derived(
    section.entries.length > 0
      ? section.entries.map((e) => `${e.key} = ${e.value}`).join("\n")
      : "",
  );
</script>

<Card class="rounded-lg" data-testid="config-file-card-{section.name}">
  <Button variant="ghost" size="sm"
    type="button"
    class="flex w-full items-center justify-between gap-3 p-4 text-left hover:bg-muted/30 rounded-lg"
    onclick={() => (expanded = !expanded)}
    aria-expanded={expanded}
    data-testid="config-file-card-toggle-{section.name}"
  >
    <div class="flex items-center gap-2 min-w-0 flex-1">
      {#if expanded}
        <ChevronDown class="h-4 w-4 text-muted-foreground shrink-0" />
      {:else}
        <ChevronRight class="h-4 w-4 text-muted-foreground shrink-0" />
      {/if}
      <div class="min-w-0 flex-1">
        <div class="flex items-center gap-2">
          <h3 class="text-sm font-medium truncate">{section.name}</h3>
          {#if status === "exists"}
            <span class="inline-flex items-center gap-1 rounded-full bg-success/10 px-2 py-0.5 text-xs text-success-foreground">
              <CheckCircle2 class="h-3 w-3" />
              {$t("settings.config.status_exists")}
            </span>
          {:else}
            <span class="inline-flex items-center gap-1 rounded-full bg-warning/10 px-2 py-0.5 text-xs text-warning-foreground">
              <FileX class="h-3 w-3" />
              {$t("settings.config.status_missing")}
            </span>
          {/if}
        </div>
        {#if section.description}
          <p class="mt-0.5 text-xs text-muted-foreground truncate">{section.description}</p>
        {/if}
      </div>
    </div>
  </Button>

  {#if expanded}
    <div class="border-t border-border/50 p-4 space-y-3">
      {#if filePath}
        <div class="flex items-center justify-between gap-2 text-xs">
          <code
            class="font-mono text-muted-foreground truncate"
            title={filePath}
          >{filePath}</code>
          <Button variant="ghost" size="sm"
            type="button"
            class="inline-flex items-center gap-1 rounded-md px-2 py-1 text-muted-foreground hover:bg-muted hover:text-foreground"
            onclick={copyPath}
            data-testid="config-file-card-copy-{section.name}"
          >
            <Copy class="h-3 w-3" />
            {$t("settings.config.copy_path")}
          </Button>
        </div>
      {/if}

      {#if !fileExists}
        <div class="flex items-center justify-between gap-2 rounded-md border border-warning/30 bg-warning/5 px-3 py-2 text-sm">
          <span class="inline-flex items-center gap-2 text-warning-foreground">
            <AlertCircle class="h-4 w-4" />
            {$t("settings.config.missing_hint")}
          </span>
        </div>
      {:else if section.redirect_route}
        <Button
          variant="outline"
          size="sm"
          onclick={gotoRedirect}
          data-testid="config-file-card-redirect-{section.name}"
        >
          <ExternalLink class="h-3.5 w-3.5 mr-1.5" />
          {$t("settings.see_details")}
        </Button>
      {:else if section.entries.length > 0}
        <div class="space-y-1.5" data-testid="config-file-card-entries-{section.name}">
          {#each section.entries as entry (entry.key)}
            <div class="grid grid-cols-2 gap-2 text-sm">
              <span class="text-muted-foreground">{entry.key}</span>
              <span class="font-mono text-foreground break-all">{entry.value}</span>
            </div>
          {/each}
        </div>

        <Button variant="ghost" size="sm"
          type="button"
          class="text-xs text-muted-foreground hover:text-foreground"
          onclick={() => (rawOpen = !rawOpen)}
        >
          {rawOpen ? $t("settings.config.hide_raw") : $t("settings.config.show_raw")}
        </Button>
        {#if rawOpen}
          <pre class="rounded bg-muted/50 p-2 text-xs font-mono overflow-x-auto"><code>{rawValue}</code></pre>
        {/if}
      {:else}
        <p class="text-xs text-muted-foreground italic">{$t("settings.config.empty")}</p>
      {/if}
    </div>
  {/if}
</Card>
