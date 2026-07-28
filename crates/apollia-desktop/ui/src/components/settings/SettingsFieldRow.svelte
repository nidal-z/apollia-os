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
   *   control      required snippet. The right-aligned control(s).
   *   for          optional. Associates the label with the control's `id`.
   *   layout       "row" (default) keeps label + control on one line and wraps
   *                the control below on narrow widths; "stack" always places the
   *                control under the label (long FR labels / full-width fields).
   *   border       bottom hairline between rows (default true).
   *   data-testid  forwarded to the row wrapper.
   */
  import type { Snippet } from "svelte";
  import { cn } from "$lib/utils";

  interface Props {
    label: string;
    hint?: string;
    control: Snippet;
    for?: string;
    layout?: "row" | "stack";
    border?: boolean;
    "data-testid"?: string;
  }

  let {
    label,
    hint,
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
  <div class="min-w-0 flex-1">
    <label for={htmlFor} class="block text-label-md text-foreground">{label}</label>
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
