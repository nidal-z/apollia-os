<script lang="ts">
  import type { Icon } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";

  let {
    icon: IconComponent,
    title,
    subtitle,
    ctaLabel,
    ctaAction,
    page,
  }: {
    icon: typeof Icon;
    title: string;
    subtitle?: string;
    ctaLabel?: string;
    ctaAction?: () => void;
    page?: string;
  } = $props();
</script>

<div
  class="flex flex-col items-center justify-center gap-4 rounded-2xl py-16 glass-card glass-border"
  data-testid={page ? `empty-state-${page}` : undefined}
>
  <div class="flex h-16 w-16 items-center justify-center rounded-2xl bg-gradient-to-br from-primary/10 to-secondary/5">
    <IconComponent size={28} class="text-primary/40" />
  </div>
  <p class="text-base font-medium text-muted-foreground">{title}</p>
  {#if subtitle}
    <p class="max-w-md text-center text-sm text-muted-foreground/60">{subtitle}</p>
  {/if}
  {#if ctaLabel && ctaAction}
    <Button variant="primary-gradient" onclick={ctaAction} class="mt-2">{ctaLabel}</Button>
  {/if}
</div>
