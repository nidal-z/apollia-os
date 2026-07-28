/**
 * List motion presets.
 *
 * Canonical parameters for animated lists: FLIP reordering plus row
 * enter/leave. They keep every keyed-each list in the app moving to the same
 * physics and collapse to an instant step under reduced motion, via `rm()`
 * from `./motion`.
 *
 * The runtime easing is `cubicOut` from `svelte/easing`, the JS analogue of
 * the CSS `--ease-apple` decelerate curve (`easing.apple` in `./motion`):
 * `svelte/animate` and `svelte/transition` take an easing function, not a
 * cubic-bezier tuple, so the tuple cannot be passed through directly.
 *
 * Canonical usage - a keyed each with FLIP reorder and row transitions:
 *
 *   <script lang="ts">
 *     import { flip } from "svelte/animate";
 *     import { fly } from "svelte/transition";
 *     import { listFlip, rowIn, rowOut } from "$lib/design/listMotion";
 *   </script>
 *
 *   {#each items as item (item.id)}
 *     <li
 *       animate:flip={listFlip()}
 *       in:fly={rowIn()}
 *       out:fly={rowOut()}
 *     >
 *       {item.label}
 *     </li>
 *   {/each}
 */
import type { FlipParams } from "svelte/animate";
import type { FlyParams } from "svelte/transition";
import { cubicOut } from "svelte/easing";
import { duration, rm } from "$lib/design/motion";

/** FLIP params for `animate:flip` on list reordering. Instant under reduced motion. */
export function listFlip(): FlipParams {
  return rm({ duration: duration.base, easing: cubicOut });
}

/** Snappier FLIP for direct drag / manual reordering, where latency reads as lag. */
export function reorderFlip(): FlipParams {
  return rm({ duration: duration.fast, easing: cubicOut });
}

/** `in:fly` params for a row entering: rises 8px while fading in. */
export function rowIn(): FlyParams {
  return rm({ y: 8, opacity: 0, duration: duration.base, easing: cubicOut });
}

/** `out:fly` params for a row leaving: shorter, so removals feel immediate. */
export function rowOut(): FlyParams {
  return rm({ y: 8, opacity: 0, duration: duration.fast, easing: cubicOut });
}
