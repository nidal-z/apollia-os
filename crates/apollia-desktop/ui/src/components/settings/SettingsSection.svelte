<script lang="ts">
  /**
   * SettingsSection - a titled card that groups related settings field rows.
   *
   * The canonical section card of the settings sub-page frame: a bordered `card`
   * surface with a header (optional leading icon glyph, uppercase title,
   * optional description, optional right-aligned `actions`), a body that stacks
   * the field rows, and an optional `footer` slot for section-scoped controls.
   *
   * Contract (imported by every settings sub-page):
   *
   *   <SettingsSection title={$t('...')} description={$t('...')} bodyLayout="flush">
   *     {#snippet icon()}<Cpu size={15} strokeWidth={1.75} />{/snippet}
   *     {#snippet actions()}<Button .../>{/snippet}
   *     <SettingsFieldRow ... />
   *     <SettingsFieldRow ... />
   *   </SettingsSection>
   *
   * Props:
   *   title        required. Section title (uppercase, already translated).
   *   description  optional. One-line description under the title.
   *   icon         optional snippet. Small leading glyph (lucide).
   *   actions      optional snippet. Right-aligned header actions.
   *   footer       optional snippet. Section-scoped footer row.
   *   bodyLayout   "stack" (default) gaps direct children with `space-y-4` for
   *                free-form content; "flush" removes the gap so stacked
   *                <SettingsFieldRow> hairlines butt together like the mockup.
   *   children     required. The section body (field rows or free-form content).
   *   data-testid  forwarded to the section element.
   */
  import type { Snippet } from "svelte";

  interface Props {
    title: string;
    description?: string;
    icon?: Snippet;
    actions?: Snippet;
    footer?: Snippet;
    bodyLayout?: "stack" | "flush";
    children: Snippet;
    "data-testid"?: string;
  }

  let {
    title,
    description,
    icon,
    actions,
    footer,
    bodyLayout = "stack",
    children,
    ...rest
  }: Props = $props();
</script>

<section class="overflow-hidden rounded-xl border border-border bg-card shadow-sm" {...rest}>
  <header class="flex items-start justify-between gap-3.5 px-4 pb-1 pt-4">
    <div class="flex items-start gap-3">
      {#if icon}
        <span
          class="mt-px flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-muted-foreground"
        >
          {@render icon()}
        </span>
      {/if}
      <div class="space-y-0.5">
        <h3 class="text-overline uppercase text-muted-foreground">{title}</h3>
        {#if description}
          <p class="text-caption text-muted-foreground/80">{description}</p>
        {/if}
      </div>
    </div>
    {#if actions}
      <div class="shrink-0">{@render actions()}</div>
    {/if}
  </header>

  <div class={bodyLayout === "flush" ? "px-4 pb-1 pt-2" : "space-y-4 px-4 pb-4 pt-3"}>
    {@render children()}
  </div>

  {#if footer}
    <footer class="mt-1 flex items-center gap-3 border-t border-border/60 px-4 py-3">
      {@render footer()}
    </footer>
  {/if}
</section>
