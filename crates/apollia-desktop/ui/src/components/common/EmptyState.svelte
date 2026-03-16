<script lang="ts">
  import type { Icon } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";

  let {
    icon: IconComponent,
    title,
    subtitle,
    ctaLabel,
    ctaAction,
  }: {
    icon: typeof Icon;
    title: string;
    subtitle?: string;
    ctaLabel?: string;
    ctaAction?: () => void;
  } = $props();
</script>

<div class="flex flex-col items-center justify-center gap-4 rounded-lg border border-dashed py-16">
  <IconComponent size={48} class="text-muted-foreground/40" />
  <p class="text-lg font-medium text-muted-foreground">{title}</p>
  {#if subtitle}
    <p class="max-w-md text-center text-sm text-muted-foreground/60">{subtitle}</p>
  {/if}
  {#if ctaLabel && ctaAction}
    <Button variant="outline" onclick={ctaAction}>{ctaLabel}</Button>
  {/if}
</div>
