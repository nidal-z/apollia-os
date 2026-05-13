<script lang="ts">
  import { cn } from "$lib/utils";
  import { Loader2 } from "lucide-svelte";
  import type { HTMLAttributes } from "svelte/elements";

  type Variant = "inline" | "centered";

  interface Props extends Omit<HTMLAttributes<HTMLSpanElement>, "class"> {
    size?: number;
    variant?: Variant;
    label?: string;
    class?: string;
  }

  let {
    size = 16,
    variant = "inline",
    label,
    class: className = "",
    ...rest
  }: Props = $props();
</script>

<span
  role="status"
  aria-label={label ?? "Loading"}
  {...rest}
  class={cn(
    variant === "centered"
      ? "absolute inset-0 flex items-center justify-center"
      : "inline-flex items-center justify-center",
    className,
  )}
>
  <Loader2 {size} class="spinner-icon animate-spin text-current" aria-hidden="true" />
  {#if label}<span class="sr-only">{label}</span>{/if}
</span>

<style>
  @media (prefers-reduced-motion: reduce) {
    :global(.spinner-icon) {
      animation-duration: 3s !important;
    }
  }
</style>
