<script lang="ts">
  /**
   * BrandLogo - the mark of a connection on a neutral tile, the same tile for
   * every one: the logo of its service in one colour, the colour of the text,
   * so it reads in the light and the dark theme alike; else the icon of its
   * kind; else the initials of its name, as "Ab". It is decorative, the name
   * always stands beside it.
   */
  import type { Snippet } from "svelte";
  import { cn } from "$lib/utils";
  import { serviceInitials } from "$lib/connections/brandLogo";

  interface Props {
    /** The glyph, from `brandLogoPath`; null or absent falls back to the icon, then the initials. */
    path?: string | null;
    /** The name of the connection, whose initials stand when there is no glyph nor icon. */
    name?: string;
    /** The icon of a kind (a folder, a mailbox), drawn when there is no glyph. */
    icon?: Snippet;
    /** md = add window row (36px), list = connection list (32px), lg = card (40px). */
    size?: "md" | "list" | "lg";
    "data-testid"?: string;
  }

  let { path = null, name = "", icon, size = "md", "data-testid": testid }: Props = $props();

  const tileClass = $derived(
    size === "lg" ? "h-10 w-10 rounded-lg" : size === "list" ? "h-8 w-8 rounded-lg" : "h-9 w-9 rounded-lg",
  );
  const glyphClass = $derived(size === "lg" ? "h-5 w-5" : "h-4 w-4");
  const initialsClass = $derived(size === "lg" ? "text-sm" : "text-xs");
</script>

<span
  class={cn(
    "inline-flex shrink-0 select-none items-center justify-center border border-border/70 bg-muted text-foreground",
    tileClass,
  )}
  data-testid={testid}
  data-mark={path ? "logo" : icon ? "icon" : "initials"}
>
  {#if path}
    <svg class={glyphClass} viewBox="0 0 24 24" fill="currentColor" aria-hidden="true" focusable="false">
      <path d={path} />
    </svg>
  {:else if icon}
    {@render icon()}
  {:else}
    <span class={cn("font-semibold leading-none tracking-tight text-foreground/85", initialsClass)} aria-hidden="true">
      {serviceInitials(name)}
    </span>
  {/if}
</span>
