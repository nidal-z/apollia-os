<script lang="ts">
  /**
   * Right-side sheet showing the full template detail.
   *
   * Loads the full body via `templates_get` on open (never imported eagerly
   * — the body can be non-trivial). Builder mode gets an extra "Show code"
   * toggle that flips the description area to a raw-TOML viewer.
   */
  import { t } from "svelte-i18n";
  import { Sheet } from "$lib/components/ui/sheet";
  import { uiMode } from "$lib/stores/mode";
  import { getTemplate, type TemplateFull, type TemplateMeta } from "$lib/templates/registry";
  import TemplateCategoryBadge from "./TemplateCategoryBadge.svelte";

  interface Props {
    template: TemplateMeta | null;
    onclose: () => void;
    onuse: (template: TemplateMeta) => void;
  }

  let { template, onclose, onuse }: Props = $props();

  let full = $state<TemplateFull | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let showCode = $state(false);

  $effect(() => {
    if (!template) {
      full = null;
      showCode = false;
      return;
    }
    loading = true;
    error = null;
    getTemplate(template.id)
      .then((data) => {
        full = data;
      })
      .catch((e) => {
        error = e instanceof Error ? e.message : String(e);
      })
      .finally(() => {
        loading = false;
      });
  });
</script>

<Sheet open={template !== null} {onclose} width="lg">
  {#if template}
    <div class="flex h-full flex-col" data-testid="template-detail-sheet">
      <header class="border-b border-border/60 px-5 py-4">
        <h2 class="text-lg font-semibold">{template.title}</h2>
        <p class="mt-1 text-xs text-muted-foreground">{template.author}</p>
        <div class="mt-2 flex flex-wrap gap-1.5">
          <TemplateCategoryBadge
            kind="category"
            value={template.category}
            label={$t(`templates.category.${template.category}`)}
          />
          <TemplateCategoryBadge
            kind="difficulty"
            value={template.difficulty}
            label={$t(`templates.difficulty.${template.difficulty}`)}
          />
          <TemplateCategoryBadge
            kind="source"
            value={template.source}
            label={template.source === "official"
              ? $t("templates.source.official")
              : $t("templates.source.community")}
          />
        </div>
      </header>

      <div class="flex-1 overflow-y-auto px-5 py-4">
        <p class="text-sm text-foreground">{template.description}</p>

        {#if template.dependencies.length > 0}
          <section class="mt-5">
            <h3 class="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground/70">
              {$t("templates.detail.dependencies")}
            </h3>
            <ul class="mt-2 space-y-1 text-xs text-muted-foreground">
              {#each template.dependencies as dep}
                <li class="flex items-center gap-2">
                  <span class="h-1.5 w-1.5 rounded-full bg-primary"></span>
                  {$t("templates.detail.requires", { values: { name: dep } })}
                </li>
              {/each}
            </ul>
          </section>
        {/if}

        {#if $uiMode === "builder"}
          <section class="mt-5">
            <button
              type="button"
              class="text-[11px] text-muted-foreground underline decoration-dotted underline-offset-2 hover:text-foreground"
              onclick={() => (showCode = !showCode)}
              data-testid="template-detail-toggle-code"
            >
              {showCode
                ? $t("templates.detail.hide_code")
                : $t("templates.detail.show_code")}
            </button>
            {#if showCode}
              {#if loading}
                <p class="mt-2 text-xs text-muted-foreground">{$t("common.loading")}</p>
              {:else if error}
                <p class="mt-2 text-xs text-destructive">{error}</p>
              {:else if full}
                <pre
                  class="mt-2 max-h-80 overflow-auto rounded-md border border-border bg-muted/40 p-3 text-[11px] leading-relaxed"
                  data-testid="template-detail-code">{full.body}</pre>
              {/if}
            {/if}
          </section>
        {/if}
      </div>

      <footer class="flex items-center justify-end gap-2 border-t border-border/60 px-5 py-3">
        <button
          type="button"
          class="rounded-md border border-border px-3 py-1.5 text-xs text-muted-foreground hover:border-muted-foreground hover:text-foreground"
          onclick={onclose}
        >
          {$t("common.cancel")}
        </button>
        <button
          type="button"
          class="rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground hover:bg-primary/90"
          onclick={() => onuse(template)}
          data-testid="template-detail-use"
        >
          {$t("templates.card.use_cta")}
        </button>
      </footer>
    </div>
  {/if}
</Sheet>
