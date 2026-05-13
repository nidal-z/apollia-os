<script lang="ts">
  import { cn } from "$lib/utils";
  import { Check, type Icon } from "lucide-svelte";

  interface Step {
    label: string;
    description?: string;
    icon?: typeof Icon;
  }

  interface Props {
    steps: Step[];
    current?: number;
    orientation?: "horizontal" | "vertical";
    class?: string;
  }

  let {
    steps,
    current = $bindable(0),
    orientation = "horizontal",
    class: className = "",
  }: Props = $props();

  function stateOf(index: number): "completed" | "active" | "upcoming" {
    if (index < current) return "completed";
    if (index === current) return "active";
    return "upcoming";
  }
</script>

<ol
  class={cn(
    orientation === "horizontal"
      ? "grid gap-2"
      : "flex flex-col gap-4",
    className,
  )}
  style={orientation === "horizontal"
    ? `grid-template-columns: repeat(${steps.length}, minmax(0, 1fr));`
    : undefined}
  aria-label="Progress"
>
  {#each steps as step, i}
    {@const state = stateOf(i)}
    {@const Icon = step.icon}
    <li
      class={cn(
        "relative flex gap-3 transition-all duration-200",
        orientation === "horizontal" ? "flex-col items-center text-center" : "flex-row items-start",
      )}
      aria-current={state === "active" ? "step" : undefined}
    >
      <!-- Connector line -->
      {#if i < steps.length - 1}
        <span
          aria-hidden="true"
          class={cn(
            "absolute",
            orientation === "horizontal"
              ? "top-4 left-1/2 h-0.5 w-full"
              : "left-4 top-8 h-[calc(100%_+_0.5rem)] w-0.5",
            state === "completed" ? "bg-primary" : "bg-border",
          )}
        ></span>
      {/if}

      <!-- Indicator
           NB. The `ring-offset-color` token is inherited so the focus halo
           around the active step blends with whatever container it lives in
           (Dialog uses `bg-card`, the page may use `bg-background`).
           Hard-coding `ring-offset-background` produced a 2px white sliver
           around the active circle whenever the parent was `bg-card`. -->
      <span
        class={cn(
          "relative z-10 inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-full border text-xs font-medium transition-[background-color,transform,opacity] duration-200",
          state === "completed" && "border-primary bg-primary text-primary-foreground",
          state === "active" &&
            "border-primary bg-card text-primary ring-2 ring-primary/30 scale-105",
          state === "upcoming" && "border-border bg-muted text-muted-foreground",
        )}
      >
        {#if state === "completed"}
          <Check size={16} strokeWidth={2.5} aria-hidden="true" />
        {:else if Icon}
          <Icon size={16} aria-hidden="true" />
        {:else}
          {i + 1}
        {/if}
      </span>

      <!-- Label + description -->
      <span
        class={cn(
          "flex flex-col gap-0.5",
          orientation === "horizontal" ? "items-center" : "items-start",
        )}
      >
        <span
          class={cn(
            "text-sm font-medium",
            state === "upcoming" ? "text-muted-foreground" : "text-foreground",
          )}
        >
          {step.label}
        </span>
        {#if step.description}
          <span class="text-xs text-muted-foreground">{step.description}</span>
        {/if}
      </span>
    </li>
  {/each}
</ol>
