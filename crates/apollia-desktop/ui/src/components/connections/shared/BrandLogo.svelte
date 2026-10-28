<script lang="ts">
  /**
   * BrandLogo - the logo of a service on a neutral tile, in one colour: the
   * colour of the text, so it reads in the light and the dark theme alike. It
   * is decorative, the name of the service always stands beside it.
   */
  import { cn } from "$lib/utils";

  interface Props {
    /** The glyph, from `brandLogoPath`. */
    path: string;
    /** md = add window row (36px), list = connection list (32px), lg = card (40px). */
    size?: "md" | "list" | "lg";
    "data-testid"?: string;
  }

  let { path, size = "md", "data-testid": testid }: Props = $props();

  const tileClass = $derived(
    size === "lg" ? "h-10 w-10 rounded-lg" : size === "list" ? "h-8 w-8 rounded-lg" : "h-9 w-9 rounded-md",
  );
  const glyphClass = $derived(size === "lg" ? "h-5 w-5" : "h-4 w-4");
</script>

<span
  class={cn("inline-flex shrink-0 items-center justify-center bg-muted text-foreground", tileClass)}
  data-testid={testid}
>
  <svg class={glyphClass} viewBox="0 0 24 24" fill="currentColor" aria-hidden="true" focusable="false">
    <path d={path} />
  </svg>
</span>
