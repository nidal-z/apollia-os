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
  class="relative flex flex-col items-center justify-center gap-4 overflow-hidden rounded-2xl py-16 glass-card glass-border"
  data-testid={page ? `empty-state-${page}` : undefined}
>
  <!-- Accent wash — sits behind the icon for a soft brand presence. -->
  <div class="pointer-events-none absolute inset-0 bg-gradient-accent opacity-60" aria-hidden="true"></div>
  <div class="relative flex h-16 w-16 items-center justify-center rounded-2xl bg-gradient-primary shadow-primary-md">
    <IconComponent size={28} class="text-white/90" />
  </div>
  <p class="relative text-base font-medium text-muted-foreground">{title}</p>
  {#if subtitle}
    <p class="relative max-w-md text-center text-sm text-muted-foreground/60">{subtitle}</p>
  {/if}
  {#if ctaLabel && ctaAction}
    <Button variant="primary-gradient" onclick={ctaAction} class="relative mt-2">{ctaLabel}</Button>
  {/if}
</div>
