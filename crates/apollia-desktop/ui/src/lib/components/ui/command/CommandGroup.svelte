<script lang="ts">
  interface Props {
    label: string;
    children?: import("svelte").Snippet;
  }

  let { label, children }: Props = $props();
</script>

<div role="group" aria-label={label} class="py-1">
  <div
    class="sticky top-0 z-[1] bg-card/95 px-2.5 pb-1 pt-1 text-[10px] font-medium uppercase tracking-wider text-muted-foreground backdrop-blur-sm"
  >
    {label}
  </div>
  {@render children?.()}
</div>
