<script lang="ts">
  import { cn } from "$lib/utils";

  interface Props {
    value?: string;
    onchange?: (value: string) => void;
    class?: string;
    children?: import("svelte").Snippet;
    "data-testid"?: string;
  }

  let {
    value = $bindable(""),
    onchange,
    class: className = "",
    children,
    ...restProps
  }: Props = $props();
</script>

<div
  role="radiogroup"
  class={cn("flex flex-col gap-2", className)}
  {...restProps}
>
  {@render children?.()}
</div>
