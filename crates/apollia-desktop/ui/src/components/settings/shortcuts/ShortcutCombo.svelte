<script lang="ts">
  /**
   * Renders a full chord as premium keycaps. Handles `+` separators, `X / Y`
   * alternatives, and per-platform glyphs (mac symbols vs Win/Linux words) via
   * the catalog's `comboFor`. One `aria-label` on the wrapper carries the
   * spoken form so the individual caps stay decorative.
   */
  import { comboFor, splitCombo, type ShortcutEntry } from "$lib/navigation/shortcuts";
  import Keycap from "./Keycap.svelte";

  interface Props {
    entry: ShortcutEntry;
    mac: boolean;
    ariaLabel: string;
  }

  let { entry, mac, ariaLabel }: Props = $props();

  const MODIFIERS = new Set([
    "⌘",
    "⇧",
    "⌥",
    "⌃",
    "Cmd",
    "Command",
    "Ctrl",
    "Control",
    "Alt",
    "Option",
    "Shift",
    "Win",
  ]);

  let alternatives = $derived(
    comboFor(entry, mac)
      .split("/")
      .map((alt) => alt.trim())
      .filter(Boolean),
  );
</script>

<span class="inline-flex flex-shrink-0 flex-wrap items-center gap-1.5" aria-label={ariaLabel}>
  {#each alternatives as alt, ai (ai)}
    {#if ai > 0}
      <span class="px-0.5 text-caption text-muted-foreground/50" aria-hidden="true">/</span>
    {/if}
    {#each splitCombo(alt) as token, ti (ti)}
      {#if ti > 0}
        <span class="text-caption text-muted-foreground/40" aria-hidden="true">+</span>
      {/if}
      <Keycap
        label={token}
        variant={MODIFIERS.has(token) ? "modifier" : "key"}
        wide={token.length > 1}
      />
    {/each}
  {/each}
</span>
