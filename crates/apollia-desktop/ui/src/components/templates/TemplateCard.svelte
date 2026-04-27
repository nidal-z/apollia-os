<script lang="ts">
  /**
   * Card rendered in the template gallery grid.
   *
   * Shows the user-facing title, a 1-2 sentence description, and three
   * chips (category / difficulty / source). The hover preview surfaces
   * a short dependency list so the operator can tell at a glance whether
   * the template is actionable.
   */
  import { t } from "svelte-i18n";
  import { Sparkles, Bot, GitBranch, Timer } from "lucide-svelte";
  import type { TemplateMeta } from "$lib/templates/registry";
  import TemplateCategoryBadge from "./TemplateCategoryBadge.svelte";

  interface Props {
    template: TemplateMeta;
    onselect: (template: TemplateMeta) => void;
    onuse: (template: TemplateMeta) => void;
  }

  let { template, onselect, onuse }: Props = $props();

  const icon = $derived.by(() => {
    if (template.kind === "automation") return Timer;
    if (template.kind === "agent") return Bot;
    return GitBranch;
  });

  function handleUse(event: MouseEvent) {
    event.stopPropagation();
    onuse(template);
  }
</script>

<button
  type="button"
  class="group relative flex h-full w-full flex-col rounded-lg border border-border bg-card p-4 text-left transition-all hover:border-primary/50 hover:shadow-sm focus:outline-none focus-visible:ring-2 focus-visible:ring-primary"
  onclick={() => onselect(template)}
  data-testid="template-card-{template.id}"
>
  <div class="flex items-start gap-3">
    <div class="mt-0.5 flex h-9 w-9 shrink-0 items-center justify-center rounded-md bg-primary/10 text-primary">
      {#if icon}
        {@const Icon = icon}
        <Icon size={18} strokeWidth={1.75} />
      {:else}
        <Sparkles size={18} strokeWidth={1.75} />
      {/if}
    </div>
    <div class="flex-1 min-w-0">
      <h3 class="truncate text-sm font-semibold text-foreground">{template.title}</h3>
      <p class="mt-0.5 text-xs text-muted-foreground line-clamp-2">{template.description}</p>
    </div>
  </div>

  <div class="mt-3 flex flex-wrap gap-1.5">
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

  <!-- Hover preview — only a short hint, keeps the card tight. -->
  <div class="mt-3 text-[11px] text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100">
    {#if template.dependencies.length > 0}
      {$t("templates.preview.uses", {
        values: { list: template.dependencies.slice(0, 3).join(", ") },
      })}
    {:else}
      {$t("templates.preview.no_dependencies")}
    {/if}
  </div>

  <div class="mt-auto flex items-center justify-between pt-4">
    <span class="text-[10px] text-muted-foreground/70">{template.author}</span>
    <span
      class="inline-flex items-center gap-1 rounded-md bg-primary px-2.5 py-1 text-[11px] font-medium text-primary-foreground"
      onclick={handleUse}
      onkeydown={(e) => (e.key === "Enter" || e.key === " ") && handleUse(e as unknown as MouseEvent)}
      role="button"
      tabindex="0"
      data-testid="template-card-use-{template.id}"
    >
      {$t("templates.card.use_cta")}
    </span>
  </div>
</button>
