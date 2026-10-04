import { describe, test, expect } from "vitest";
import {
  CHAT_SEND_KEY_DEFAULT,
  composerEnterAction,
  parseChatSendKey,
  type ComposerKey,
} from "./chatSendKey";

function key(partial: Partial<ComposerKey>): ComposerKey {
  return { key: "Enter", shiftKey: false, metaKey: false, ctrlKey: false, ...partial };
}

const ENTER = key({});
const SHIFT_ENTER = key({ shiftKey: true });
const CMD_ENTER = key({ metaKey: true });
const CTRL_ENTER = key({ ctrlKey: true });

describe("chat composer - the send key", () => {
  test("by default Enter sends and Shift + Enter breaks the line", () => {
    // GIVEN nothing stored
    const mode = parseChatSendKey(null);

    // WHEN Enter and Shift + Enter are pressed in the composer
    const enter = composerEnterAction(ENTER, mode);
    const shiftEnter = composerEnterAction(SHIFT_ENTER, mode);

    // THEN Enter sends and Shift + Enter breaks the line
    expect(mode).toBe("enter");
    expect(CHAT_SEND_KEY_DEFAULT).toBe("enter");
    expect(enter).toBe("send");
    expect(shiftEnter).toBe("newline");
  });

  test("the other choice sends with Cmd or Ctrl + Enter and breaks the line on Enter", () => {
    // GIVEN the composer set to send with the modifier
    const mode = parseChatSendKey("mod-enter");

    // WHEN each combination is pressed
    const actions = [ENTER, SHIFT_ENTER, CMD_ENTER, CTRL_ENTER].map((e) =>
      composerEnterAction(e, mode),
    );

    // THEN Enter and Shift + Enter break the line, Cmd and Ctrl + Enter send
    expect(actions).toEqual(["newline", "newline", "send", "send"]);
  });

  test("a key typed while an input method composes never sends", () => {
    // GIVEN Enter pressed to validate a composed character
    const composing = key({ isComposing: true });

    // WHEN the composer reads it under each choice
    const actions = [composerEnterAction(composing, "enter"), composerEnterAction(composing, "mod-enter")];

    // THEN neither sends
    expect(actions).toEqual(["newline", "newline"]);
  });

  test("a key other than Enter is not the composer's to decide, and an unknown stored value reads as the default", () => {
    // GIVEN a letter, and a stored value no version wrote
    const letter = key({ key: "a" });

    // WHEN they are read
    const action = composerEnterAction(letter, "enter");
    const mode = parseChatSendKey("shift-enter");

    // THEN the letter is left alone and the default stands
    expect(action).toBeNull();
    expect(mode).toBe("enter");
  });
});
