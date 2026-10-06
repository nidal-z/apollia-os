<script lang="ts">
  /**
   * SettingsFieldRow - one label + control + hint line inside a SettingsSection.
   *
   * The canonical field row of the settings sub-page frame. The left column
   * holds a label and an optional hint; the `control` snippet renders the
   * interactive control on the right (Input, Toggle, segmented control, Select,
   * Button). A bottom hairline separates stacked rows; the last row in a stack
   * drops it automatically (`last:border-b-0`).
   *
   * Contract (imported by every settings sub-page):
   *
   *   <SettingsFieldRow label={$t('...')} hint={$t('...')} for="field-id">
   *     {#snippet control()}
   *       <Input id="field-id" bind:value={...} />
   *     {/snippet}
   *   </SettingsFieldRow>
   *
   * Props:
   *   label        required. Field label text (already translated).
   *   hint         optional. Contextual help rendered under the label.
   *   help         optional. A "?" beside the label, explained on hover.
   *   indent       optional. The row depends on the one above: set back, with
   *                a corner arrow before its label.
   *   control      required snippet. The right-aligned control(s).
   *   for          optional. Associates the label with the control's `id`.
   *   layout       "row" (default) keeps label + control on one line and wraps
   *                the control below on narrow widths; "stack" always places the
   *                control under the label (long FR labels / full-width fields).
   *   border       bottom hairline between rows (default true).
   *   data-testid  forwarded to the row wrapper.
   */
  import type { Snippet } from "svelte";
  import { CircleHelp, CornerDownRight } from "lucide-svelte";
  import { Tooltip } from "$lib/components/ui/tooltip";
  import { cn } from "$lib/utils";

  interface Props {
    label: string;
    hint?: string;
    help?: string;
    indent?: boolean;
    control: Snippet;
    for?: string;
    layout?: "row" | "stack";
    border?: boolean;
    "data-testid"?: string;
  }

  let {
    label,
    hint,
    help,
    indent = false,
    control,
    for: htmlFor,
    layout = "row",
    border = true,
    "data-testid": testid,
  }: Props = $props();
</script>

<div
  data-testid={testid}
  class={cn(
    "flex gap-5 py-3",
    layout === "stack"
      ? "flex-col items-stretch"
      : "flex-wrap items-center justify-between",
    border && "border-b border-border/60 last:border-b-0",
  )}
>
  <div class={cn("min-w-0 flex-1", indent && "pl-5")}>
    <div class="flex items-center gap-1.5">
      {#if indent}
        <CornerDownRight size={13} strokeWidth={1.75} class="flex-none text-muted-foreground" aria-hidden="true" />
      {/if}
      <label
        for={htmlFor}
        class={cn("block text-label-md", indent ? "text-muted-foreground" : "text-foreground")}
      >{label}</label>
      {#if help}
        <Tooltip content={help} side="top">
          <span
            class="inline-flex text-muted-foreground hover:text-foreground"
            data-testid={testid ? `${testid}-help` : undefined}
          >
            <CircleHelp size={13} strokeWidth={1.75} aria-hidden="true" />
            <span class="sr-only">{help}</span>
          </span>
        </Tooltip>
      {/if}
    </div>
    {#if hint}
      <p class="mt-0.5 max-w-md text-caption text-muted-foreground">{hint}</p>
    {/if}
  </div>
  <div
    class={cn(
      "flex items-center gap-2",
      layout === "stack" ? "flex-wrap" : "flex-none",
    )}
  >
    {@render control()}
  </div>
</div>
