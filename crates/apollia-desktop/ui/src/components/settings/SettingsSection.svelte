<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    title: string;
    description?: string;
    icon?: Snippet;
    actions?: Snippet;
    footer?: Snippet;
    children: Snippet;
    "data-testid"?: string;
  }

  let { title, description, icon, actions, footer, children, ...rest }: Props = $props();
</script>

<section class="glass-card glass-border rounded-lg p-4" {...rest}>
  <header class="mb-4 flex items-start justify-between gap-3">
    <div class="flex items-start gap-3">
      {#if icon}
        <span class="mt-0.5 flex h-6 w-6 shrink-0 items-center justify-center text-muted-foreground">
          {@render icon()}
        </span>
      {/if}
      <div class="space-y-0.5">
        <h3 class="text-sm font-medium uppercase tracking-wider text-muted-foreground">{title}</h3>
        {#if description}
          <p class="text-xs text-muted-foreground/80">{description}</p>
        {/if}
      </div>
    </div>
    {#if actions}
      <div class="shrink-0">{@render actions()}</div>
    {/if}
  </header>

  <div class="space-y-4">
    {@render children()}
  </div>

  {#if footer}
    <footer class="mt-4 flex items-center gap-3 border-t border-border/40 pt-3">
      {@render footer()}
    </footer>
  {/if}
</section>
