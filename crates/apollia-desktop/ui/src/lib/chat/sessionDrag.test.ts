// @vitest-environment jsdom
import { describe, expect, test } from "vitest";
import { dropTargetOf, refusesDrag, startsDrag, wouldMove } from "./sessionDrag";

function tree(html: string): HTMLElement {
  const root = document.createElement("div");
  root.innerHTML = html;
  document.body.append(root);
  return root;
}

describe("dragging a conversation with the pointer", () => {
  test("a press becomes a drag once the pointer has travelled", () => {
    // GIVEN a press at a point
    const from = { x: 100, y: 100 };
    // WHEN the pointer trembles, then moves away
    // THEN only the second is a drag, so a click stays a click
    expect(startsDrag(from, { x: 102, y: 103 })).toBe(false);
    expect(startsDrag(from, { x: 100, y: 107 })).toBe(true);
  });

  test("the place under the pointer is the nearest one that takes a conversation", () => {
    // GIVEN a folder row, a conversation inside a folder, one out of any
    // folder, and something that takes none
    const root = tree(`
      <div data-session-drop="f-1"><button id="folder">Clients</button></div>
      <div data-session-drop="f-1"><div id="inside">A conversation</div></div>
      <div data-session-drop=""><div id="loose">Another</div></div>
      <p id="elsewhere">The header</p>`);
    // WHEN the place of each is read
    // THEN the first two are the folder, the third is no folder, the last none
    expect(dropTargetOf(root.querySelector("#folder"))).toEqual({ folderId: "f-1" });
    expect(dropTargetOf(root.querySelector("#inside"))).toEqual({ folderId: "f-1" });
    expect(dropTargetOf(root.querySelector("#loose"))).toEqual({ folderId: null });
    expect(dropTargetOf(root.querySelector("#elsewhere"))).toBeNull();
    expect(dropTargetOf(null)).toBeNull();
  });

  test("a drop changes something only on another place than its own", () => {
    // GIVEN a conversation in the folder f-1
    // WHEN it is held over its own folder, another one, no folder, and nothing
    // THEN only the other folder and no folder would move it
    expect(wouldMove({ folderId: "f-1" }, "f-1")).toBe(false);
    expect(wouldMove({ folderId: "f-2" }, "f-1")).toBe(true);
    expect(wouldMove({ folderId: null }, "f-1")).toBe(true);
    expect(wouldMove({ folderId: null }, null)).toBe(false);
    expect(wouldMove(null, "f-1")).toBe(false);
  });

  test("a press in a field being typed in never drags", () => {
    // GIVEN the field a conversation is renamed in, and the row around it
    const root = tree(`<div id="row"><input id="name" /></div>`);
    // WHEN a press lands on each
    // THEN the field keeps the pointer for its text, the row can be dragged
    expect(refusesDrag(root.querySelector("#name"))).toBe(true);
    expect(refusesDrag(root.querySelector("#row"))).toBe(false);
    expect(refusesDrag(null)).toBe(false);
  });
});
