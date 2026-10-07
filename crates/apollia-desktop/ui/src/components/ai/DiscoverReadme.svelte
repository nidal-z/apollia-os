<script lang="ts">
  /**
   * DiscoverReadme - the README of the model, read at its source when the
   * user asks for it. The text is held for as long as it is shown: hiding
   * it drops it, and showing it again reads the source again. The card mounts
   * one instance per repository, so choosing another model or closing the
   * window drops it too.
   */
  import { t } from "svelte-i18n";
  import { FileText } from "lucide-svelte";
  import { getModelReadme, type ModelReadme } from "$lib/ipc/modelHub";
  import { Button } from "$lib/components/ui/button";
  import { MarkdownContent } from "$lib/components/ui/markdown";
  import { readmeBody, readmeErrorText } from "./discoverCard";

  interface Props {
    /** The repository the README is read from, `owner/name`. */
    repo: string;
  }

  let { repo }: Props = $props();

  let readme = $state<ModelReadme | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let run = 0;

  const shown = $derived(readme !== null || loading || error !== null);

  function drop(): void {
    run += 1;
    readme = null;
    loading = false;
    error = null;
  }

  async function show(): Promise<void> {
    const mine = ++run;
    loading = true;
    error = null;
    try {
      const answer = await getModelReadme(repo);
      if (mine === run) readme = answer;
    } catch (err: unknown) {
      if (mine === run) error = readmeErrorText(err);
    } finally {
      if (mine === run) loading = false;
    }
  }
</script>

<section class="flex flex-col gap-2" data-testid="ai-discover-readme">
  <div class="flex items-center gap-2">
    <FileText size={14} class="text-muted-foreground" aria-hidden="true" />
    <h3 class="text-sm font-semibold text-foreground">{$t("ai.discover.readme.title")}</h3>
    <Button
      variant="outline"
      size="sm"
      class="ml-auto"
      aria-expanded={shown}
      onclick={() => (shown ? drop() : void show())}
      data-testid="ai-discover-readme-toggle"
    >
      {$t(shown ? "ai.discover.readme.hide" : "ai.discover.readme.show")}
    </Button>
  </div>
  {#if shown}
    <div class="flex flex-col gap-3 rounded-lg border border-border p-4">
      {#if loading}
        <p class="text-sm text-muted-foreground" data-testid="ai-discover-readme-loading">
          {$t("ai.discover.readme.loading")}
        </p>
      {:else if error !== null}
        <p class="text-sm text-foreground" data-testid="ai-discover-readme-error">
          {$t("ai.discover.readme.error", { values: { detail: error } })}
        </p>
      {:else if readme}
        <p class="text-xs text-muted-foreground" data-testid="ai-discover-readme-byline">
          {$t("ai.discover.readme.byline", {
            values: {
              repo: readme.repo,
              author: readme.author,
              license: readme.license ?? $t("ai.discover.readme.license_none"),
            },
          })}
        </p>
        <div class="rounded-md bg-muted/50 p-4" data-testid="ai-discover-readme-text">
          <MarkdownContent content={readmeBody(readme.text)} />
        </div>
        <p class="text-xs text-muted-foreground">{$t("ai.discover.readme.note")}</p>
      {/if}
    </div>
  {/if}
</section>
