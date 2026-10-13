// The pointer drag the automaton plays: press on one element, move to the
// centre of another, release there.
//
// Pointer events and not the drag and drop of HTML, which the window keeps
// for the drops of the system and never delivers to the page.

/** How long the pointer rests on the destination before the release. */
const REST_MS = 50;

function centre(el: HTMLElement): { clientX: number; clientY: number } {
  const box = el.getBoundingClientRect();
  return { clientX: box.left + box.width / 2, clientY: box.top + box.height / 2 };
}

function at(point: { clientX: number; clientY: number }): PointerEventInit {
  return { ...point, bubbles: true, cancelable: true, button: 0, pointerId: 1, isPrimary: true };
}

/** Drag `source` onto `destination`, as a hand does: one move leaves the press, one lands. */
export async function dragOnto(source: HTMLElement, destination: HTMLElement): Promise<void> {
  destination.scrollIntoView({ block: "nearest" });
  const from = centre(source);
  const to = centre(destination);
  const half = {
    clientX: (from.clientX + to.clientX) / 2,
    clientY: (from.clientY + to.clientY) / 2,
  };
  source.dispatchEvent(new PointerEvent("pointerdown", at(from)));
  window.dispatchEvent(new PointerEvent("pointermove", at(half)));
  window.dispatchEvent(new PointerEvent("pointermove", at(to)));
  await new Promise((resolve) => setTimeout(resolve, REST_MS));
  window.dispatchEvent(new PointerEvent("pointerup", at(to)));
}
