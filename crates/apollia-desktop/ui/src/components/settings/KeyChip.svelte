<script lang="ts">
  interface Props {
    label: string;
    variant?: "default" | "modifier" | "key";
    "data-testid"?: string;
  }

  let { label, variant = "default", ...rest }: Props = $props();
</script>

<kbd
  class="inline-flex h-8 min-w-[2rem] items-center justify-center rounded-md border border-border bg-background/80 px-2 text-sm font-mono font-medium text-foreground shadow-sm {variant === 'modifier' ? 'text-primary/90' : ''}"
  {...rest}
>
  {label}
</kbd>
