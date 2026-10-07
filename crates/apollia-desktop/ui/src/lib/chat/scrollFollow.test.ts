import { describe, it, expect } from "vitest";
import {
  computeScrollFollow,
  computeSendSpace,
  followsGeneration,
  sendScroll,
  distanceFromBottom,
  FOLLOW_RELEASE_PX,
  JUMP_BUTTON_PX,
} from "./scrollFollow";

describe("computeScrollFollow", () => {
  it("keeps following when the answer grows faster than the viewport catches up", () => {
    // GIVEN a thread following the bottom while a chunk of answer lands, so the
    // viewport moved down but the content grew further still
    const input = {
      scrollTop: 1000,
      previousScrollTop: 600,
      scrollHeight: 1800,
      clientHeight: 400,
      wasReleased: false,
    };
    expect(distanceFromBottom(input)).toBe(400);

    // WHEN the scroll event is resolved
    const state = computeScrollFollow(input);

    // THEN the follow stays engaged, so the rest of the answer keeps scrolling
    expect(state.userScrolledUp).toBe(false);
    expect(state.showScrollToBottom).toBe(false);
  });

  it("releases the follow when the user scrolls up", () => {
    // GIVEN a scroll that moves the viewport upward, away from the bottom
    const state = computeScrollFollow({
      scrollTop: 200,
      previousScrollTop: 1400,
      scrollHeight: 1800,
      clientHeight: 400,
      wasReleased: false,
    });

    // WHEN / THEN the follow is released and the jump button appears
    expect(state.userScrolledUp).toBe(true);
    expect(state.showScrollToBottom).toBe(true);
  });

  it("holds the release while content keeps arriving below the user", () => {
    // GIVEN an already released follow and a new chunk growing the thread,
    // without the user moving at all
    const state = computeScrollFollow({
      scrollTop: 200,
      previousScrollTop: 200,
      scrollHeight: 2400,
      clientHeight: 400,
      wasReleased: true,
    });

    // WHEN / THEN the thread does not yank itself back to the bottom
    expect(state.userScrolledUp).toBe(true);
    expect(state.showScrollToBottom).toBe(true);
  });

  it("re-engages the follow once the user comes back to the bottom", () => {
    // GIVEN a released follow and a downward scroll landing inside the
    // re-engagement band
    const state = computeScrollFollow({
      scrollTop: 1400 - FOLLOW_RELEASE_PX,
      previousScrollTop: 200,
      scrollHeight: 1800,
      clientHeight: 400,
      wasReleased: true,
    });

    // WHEN / THEN the thread follows again
    expect(state.userScrolledUp).toBe(false);
    expect(state.showScrollToBottom).toBe(false);
  });

  it("shows the jump button only past its own threshold", () => {
    // GIVEN an upward scroll landing between the two thresholds
    const distance = (JUMP_BUTTON_PX + FOLLOW_RELEASE_PX) / 2;
    const state = computeScrollFollow({
      scrollTop: 1400 - distance,
      previousScrollTop: 1400,
      scrollHeight: 1800,
      clientHeight: 400,
      wasReleased: false,
    });

    // WHEN / THEN the follow is released but the button stays hidden
    expect(state.userScrolledUp).toBe(true);
    expect(state.showScrollToBottom).toBe(false);
  });

  it("ignores a sub-pixel upward jitter", () => {
    // GIVEN a scroll that drifts up by less than a pixel while far from the
    // bottom, the shape elastic overscroll and fractional offsets produce
    const state = computeScrollFollow({
      scrollTop: 999.6,
      previousScrollTop: 1000,
      scrollHeight: 1800,
      clientHeight: 400,
      wasReleased: false,
    });

    // WHEN / THEN the follow survives the jitter
    expect(state.userScrolledUp).toBe(false);
  });
});

describe("computeSendSpace", () => {
  it("leaves the room the sent message needs to reach the top", () => {
    // GIVEN a 600 px viewport with 20 px of padding on each side, and a sent
    // message 80 px tall at the end of a long thread
    const input = { clientHeight: 600, anchorTop: 2000, contentBottom: 2080, inset: 40 };

    // WHEN the room under the thread is computed
    const space = computeSendSpace(input);

    // THEN it is what the message does not fill, so that the furthest scroll
    // (content, room and bottom padding, less the viewport) lands the message
    // right under the top padding
    expect(space).toBe(480);
    const maxScrollTop = input.contentBottom + space + 20 - input.clientHeight;
    expect(maxScrollTop).toBe(input.anchorTop - 20);
  });

  it("shrinks as the answer arrives and is gone once the answer fills the viewport", () => {
    // GIVEN the same send, then an answer of 300 px, then one of 900 px
    const base = { clientHeight: 600, anchorTop: 2000, inset: 40 };

    // WHEN the room is computed at each stage
    const early = computeSendSpace({ ...base, contentBottom: 2080 + 300 });
    const late = computeSendSpace({ ...base, contentBottom: 2080 + 900 });

    // THEN it gave the answer its place, and never goes negative
    expect(early).toBe(180);
    expect(late).toBe(0);
  });
});

describe("sendScroll", () => {
  it("sends the message to the top by default, and to the bottom with the setting off", () => {
    // GIVEN a sent message in the thread
    // WHEN the setting is on, then off
    const on = sendScroll({ scrollSentToTop: true, hasAnchor: true });
    const off = sendScroll({ scrollSentToTop: false, hasAnchor: true });

    // THEN the thread pins the message at the top, or goes to the bottom
    expect(on).toBe("sent-to-top");
    expect(off).toBe("bottom");
  });

  it("goes to the bottom when there is no message to pin", () => {
    // GIVEN the setting on and an empty thread
    // WHEN a send is resolved
    const mode = sendScroll({ scrollSentToTop: true, hasAnchor: false });

    // THEN it falls back to the bottom
    expect(mode).toBe("bottom");
  });
});

describe("followsGeneration", () => {
  it("follows the answer by default", () => {
    // GIVEN the setting on and a thread the user left alone
    // WHEN a chunk lands
    const follows = followsGeneration({ followGeneration: true, released: false });

    // THEN the thread moves with it
    expect(follows).toBe(true);
  });

  it("stops following with the setting off", () => {
    // GIVEN "follow the message being generated" off
    // WHEN a chunk lands
    const follows = followsGeneration({ followGeneration: false, released: false });

    // THEN the thread stays where it is
    expect(follows).toBe(false);
  });

  it("stays released once the user scrolled up, whatever the setting", () => {
    // GIVEN the setting on and a user reading further up
    // WHEN a chunk lands
    const follows = followsGeneration({ followGeneration: true, released: true });

    // THEN the thread does not pull them back
    expect(follows).toBe(false);
  });
});
