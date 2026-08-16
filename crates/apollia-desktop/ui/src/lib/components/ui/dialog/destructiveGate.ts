/**
 * destructiveGate - the single door every destructive path on a row opens.
 *
 * A list row usually offers the same destruction from three places: a
 * Delete/Backspace keystroke while the row has focus, a trash button revealed
 * on hover, and a context-menu entry. Wiring each of them straight to the
 * effect means three chances to destroy something on a mistaken keystroke, and
 * none of the toasts in this product carries an undo.
 *
 * So the three paths call `requestDestruction`, which only opens a
 * confirmation. `commitDestruction` is the one function that runs the effect,
 * and it refuses while the gate is shut: a path wired directly to it still
 * cannot destroy anything the user has not confirmed.
 *
 * The state is a plain mutable object so a component can hold it in `$state`
 * and let Svelte's deep proxy track `open`.
 */

/** Whether a row's confirmation is currently on screen. */
export interface DestructiveGate {
  open: boolean;
}

/** A shut gate, the state a row starts in. */
export function createDestructiveGate(): DestructiveGate {
  return { open: false };
}

/** True for the keystrokes that request destruction on a focused row. */
export function isDestructiveKey(key: string): boolean {
  return key === "Delete" || key === "Backspace";
}

/** Opens the confirmation. Destroys nothing, whichever path called it. */
export function requestDestruction(gate: DestructiveGate): void {
  gate.open = true;
}

/** Shuts the confirmation. Destroys nothing. */
export function cancelDestruction(gate: DestructiveGate): void {
  gate.open = false;
}

/**
 * Runs `effect` only when the user has an open confirmation, then shuts it.
 *
 * Returns whether the effect ran, so a caller can tell a refused commit from a
 * failed one.
 */
export async function commitDestruction(
  gate: DestructiveGate,
  effect: () => Promise<void> | void,
): Promise<boolean> {
  if (!gate.open) return false;
  gate.open = false;
  await effect();
  return true;
}
