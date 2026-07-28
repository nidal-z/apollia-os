/**
 * Route transition presets.
 *
 * A short, restrained fly + fade for swapping route content and for the
 * master -> detail swap inside `SplitLayout`. Kept quieter than the list-row
 * transitions (`./listMotion`) so navigation feels crisp rather than showy.
 * Collapses to an instant step under reduced motion via `rm()` from `./motion`.
 *
 * The runtime easing is `cubicOut` from `svelte/easing`, the JS analogue of
 * the CSS `--ease-apple` decelerate curve (`easing.apple` in `./motion`),
 * since `svelte/transition` takes an easing function, not a bezier tuple.
 *
 * Canonical usage - keyed route content:
 *
 *   {#key routeId}
 *     <div in:fly={contentIn()} out:fly={contentOut()}>
 *       <svelte:component this={RouteComponent} />
 *     </div>
 *   {/key}
 */
import type { FlyParams } from "svelte/transition";
import { cubicOut } from "svelte/easing";
import { duration, rm } from "$lib/design/motion";

/** `in:fly` for route content / detail pane entering: rises 6px while fading in. */
export function contentIn(): FlyParams {
  return rm({ y: 6, opacity: 0, duration: duration.base, easing: cubicOut });
}

/** `out:fly` for route content / detail pane leaving: shorter, keeps swaps snappy. */
export function contentOut(): FlyParams {
  return rm({ y: 6, opacity: 0, duration: duration.fast, easing: cubicOut });
}
