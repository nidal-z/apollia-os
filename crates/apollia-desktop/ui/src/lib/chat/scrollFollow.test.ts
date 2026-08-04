import { describe, it, expect } from "vitest";
import {
  computeScrollFollow,
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
