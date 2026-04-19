<script lang="ts" module>
  export type AvatarSize = "xs" | "sm" | "md" | "lg" | "xl";

  /** Deterministic hue in [0, 360) derived from `name` — kept centralised so
   * sibling decorations (status dots, badges) can stay in sync. */
  export function avatarHue(name: string): number {
    if (!name) return 220;
    let sum = 0;
    for (let i = 0; i < name.length; i++) sum += name.charCodeAt(i);
    return sum % 360;
  }

  function initialsFrom(name: string): string {
    const trimmed = name.trim();
    if (!trimmed) return "?";
    const parts = trimmed.split(/\s+/);
    if (parts.length >= 2) {
      return (parts[0].charAt(0) + parts[parts.length - 1].charAt(0)).toUpperCase();
    }
    return trimmed.charAt(0).toUpperCase();
  }
</script>

<script lang="ts">
  import type { HTMLAttributes } from "svelte/elements";

  interface Props extends HTMLAttributes<HTMLElement> {
    name: string;
    size?: AvatarSize;
    src?: string | null;
    fallback?: string | null;
    ring?: boolean;
    class?: string;
  }

  let {
    name,
    size = "md",
    src = null,
    fallback = null,
    ring,
    class: klass = "",
    ...rest
  }: Props = $props();

  const SIZE_CLASSES: Record<AvatarSize, string> = {
    xs: "h-6 w-6 text-[10px] rounded-md",
    sm: "h-8 w-8 text-xs rounded-lg",
    md: "h-10 w-10 text-sm rounded-lg",
    lg: "h-12 w-12 text-base rounded-xl",
    xl: "h-16 w-16 text-lg rounded-xl",
  };

  const defaultRing: Record<AvatarSize, boolean> = {
    xs: false,
    sm: false,
    md: true,
    lg: true,
    xl: true,
  };

  const hue = $derived(avatarHue(name));
  const initials = $derived(fallback ?? initialsFrom(name));
  const showRing = $derived(ring ?? defaultRing[size]);
  const sizeClass = $derived(SIZE_CLASSES[size]);
  const ringClass = $derived(showRing ? "ring-2 ring-card ring-offset-0" : "");

  let imgFailed = $state(false);
  $effect(() => {
    void src;
    imgFailed = false;
  });
</script>

{#if src && !imgFailed}
  <img
    {src}
    alt={name}
    onerror={() => (imgFailed = true)}
    class="shrink-0 object-cover font-semibold text-white {sizeClass} {ringClass} {klass}"
    style="background: linear-gradient(135deg, hsl({hue},60%,48%) 0%, hsl({hue},60%,42%) 100%); box-shadow: 0 2px 8px -1px hsla({hue},60%,38%,0.3);"
    data-avatar-hue={hue}
    {...rest}
  />
{:else}
  <span
    role="img"
    aria-label={name}
    class="inline-flex shrink-0 items-center justify-center font-semibold text-white select-none {sizeClass} {ringClass} {klass}"
    style="background: linear-gradient(135deg, hsl({hue},60%,48%) 0%, hsl({hue},60%,42%) 100%); box-shadow: 0 2px 8px -1px hsla({hue},60%,38%,0.3);"
    data-avatar-hue={hue}
    {...rest}
  >
    {initials}
  </span>
{/if}
