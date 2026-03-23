<script lang="ts">
  import { cn } from "$lib/utils";

  interface Props {
    checked?: boolean;
    onchange?: (checked: boolean) => void;
    disabled?: boolean;
    size?: "sm" | "default";
    class?: string;
    id?: string;
    "data-testid"?: string;
    "aria-label"?: string;
  }

  let {
    checked = $bindable(false),
    onchange,
    disabled = false,
    size = "default",
    class: className = "",
    id,
    ...restProps
  }: Props = $props();

  function handleToggle() {
    if (disabled) return;
    checked = !checked;
    onchange?.(checked);
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === " " || event.key === "Enter") {
      event.preventDefault();
      handleToggle();
    }
  }

  const sizes = {
    sm: { track: "h-4 w-7", dot: "h-3 w-3", translate: "translate-x-3" },
    default: { track: "h-5 w-9", dot: "h-4 w-4", translate: "translate-x-4" },
  };
</script>

<button
  type="button"
  role="switch"
  aria-checked={checked}
  {disabled}
  {id}
  class={cn(
    "relative inline-flex shrink-0 cursor-pointer items-center rounded-full border-2 border-transparent ring-offset-background transition-colors duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40 focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50",
    checked ? "bg-primary" : "bg-border",
    sizes[size].track,
    className,
  )}
  onclick={handleToggle}
  onkeydown={handleKeydown}
  {...restProps}
>
  <span
    class={cn(
      "pointer-events-none block rounded-full bg-white shadow-sm transition-transform duration-150",
      sizes[size].dot,
      checked ? sizes[size].translate : "translate-x-0",
    )}
  ></span>
</button>
