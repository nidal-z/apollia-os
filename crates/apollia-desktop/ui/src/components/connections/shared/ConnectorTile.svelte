<script lang="ts">
  /**
   * ConnectorTile - the tokenised logo tile for a connector.
   *
   * Replaces the former inline brand gradients (`color: white` + hex) and the
   * hex palette array. The background is a semantic accent token (see
   * `accentTokenFor`), the glyph sits in the primary-foreground token, and the
   * focal selected tile opts into the `.glow-primary` halo. No hex, no raw
   * white, no inline px.
   */
  import type { Snippet } from "svelte";
  import { cn } from "$lib/utils";

  interface Props {
    /** Accent colour CSS string (from `accentTokenFor`). Defaults to primary. */
    accent?: string;
    /** Tile footprint: sm = row (24px), md = catalogue (36px), lg = lead (40px). */
    size?: "sm" | "md" | "lg";
    /** Opt-in primary halo for the focal (selected) tile. */
    glow?: boolean;
    /** Glyph rendered inside the tile (inherits currentColor). */
    icon: Snippet;
    class?: string;
  }

  let { accent = "hsl(var(--primary))", size = "md", glow = false, icon, class: className = "" }: Props =
    $props();

  const sizeClass = $derived(
    size === "sm" ? "h-6 w-6 rounded-md" : size === "lg" ? "h-10 w-10 rounded-lg" : "h-9 w-9 rounded-md",
  );
</script>

<span
  class={cn(
    "inline-flex shrink-0 items-center justify-center text-primary-foreground",
    sizeClass,
    glow && "glow-primary",
    className,
  )}
  style="background: {accent};"
>
  {@render icon()}
</span>
