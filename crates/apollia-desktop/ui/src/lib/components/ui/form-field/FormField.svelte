<script lang="ts">
  /**
   * FormField - canonical wrapper for label + control + helper/error text.
   *
   * Structure :
   *   <FormField label="..." id="..." hint="..." error="..." required>
   *     <Input id="..." ... />
   *   </FormField>
   *
   * Generates a stable `id` and ties the label/hint/error to the control via
   * aria attributes. Consumers are responsible for passing the same `id` to
   * the inner control.
   */
  import { cn } from "$lib/utils";
  import type { Snippet } from "svelte";

  interface Props {
    /** Visible label. Pass empty string to render no label. */
    label?: string;
    /** Stable id - used by the inner control + label `for` + describedby. */
    id?: string;
    /** Helper text rendered under the control (muted). */
    hint?: string;
    /** Error message rendered under the control (destructive). Suppresses hint. */
    error?: string;
    /** Visual asterisk after the label. */
    required?: boolean;
    /** Visual `(optional)` after the label. */
    optional?: boolean;
    /** Text rendered next to the label when `optional` is true. Default "(facultatif)". */
    optionalLabel?: string;
    /** Override class on the outer wrapper. */
    class?: string;
    /** Override class on the label element. */
    labelClass?: string;
    /** Render label inline (horizontal layout) instead of stacked. */
    inline?: boolean;
    /** Optional trailing content rendered next to the label (e.g. counter). */
    labelTrailing?: Snippet;
    /** Optional `data-testid` forwarded to the outer wrapper. */
    "data-testid"?: string;
    children: Snippet;
  }

  let {
    label = "",
    id,
    hint,
    error,
    required = false,
    optional = false,
    optionalLabel = "(facultatif)",
    class: className = "",
    labelClass = "",
    inline = false,
    labelTrailing,
    "data-testid": testid,
    children,
  }: Props = $props();

  const hintId = $derived(id ? `${id}-hint` : undefined);
  const errorId = $derived(id ? `${id}-error` : undefined);
</script>

<div class={cn(inline ? "flex items-center gap-3" : "space-y-1", className)} data-testid={testid}>
  {#if label}
    <div class={cn("flex items-center justify-between gap-2", inline && "shrink-0")}>
      <label
        for={id}
        class={cn(
          "text-xs font-medium text-muted-foreground",
          inline ? "shrink-0" : "block",
          labelClass,
        )}
      >
        {label}
        {#if required}
          <span class="text-destructive" aria-hidden="true">*</span>
        {:else if optional}
          <span class="ml-1 text-[10px] font-normal text-muted-foreground">{optionalLabel}</span>
        {/if}
      </label>
      {#if labelTrailing}
        {@render labelTrailing()}
      {/if}
    </div>
  {/if}

  <div class={cn(inline && "flex-1")}>
    {@render children()}
  </div>

  {#if error}
    <p id={errorId} class="text-[11px] text-destructive" role="alert">{error}</p>
  {:else if hint}
    <p id={hintId} class="text-[11px] text-muted-foreground/80">{hint}</p>
  {/if}
</div>
