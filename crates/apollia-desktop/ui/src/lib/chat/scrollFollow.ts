/**
 * Scroll-follow state for the chat thread.
 *
 * The thread follows the bottom while an answer streams, and stops following as
 * soon as the user scrolls up. Telling those two apart from container geometry
 * alone does not work: a programmatic follow also fires `scroll`, and while the
 * answer grows the container's `scrollHeight` runs ahead of the position the
 * follow is aiming at. Reading that gap as "the user scrolled up" pauses the
 * follow for the rest of the answer, which is the failure this module exists to
 * prevent.
 *
 * The discriminator is direction, not distance. A follow only ever moves the
 * viewport down; only a user moves it up. So the follow is released by an
 * upward move, held while the viewport stays far from the bottom, and re-engaged
 * as soon as the viewport comes back near it. No timers and no "is this scroll
 * mine" flag, which is what makes the decision testable on plain numbers.
 */

/** Container geometry for one scroll event, plus the position before it. */
export interface ScrollFollowInput {
  /** Current vertical scroll offset. */
  scrollTop: number;
  /** Total scrollable height, which grows while tokens arrive. */
  scrollHeight: number;
  /** Visible height of the container. */
  clientHeight: number;
  /** Offset at the previous scroll event, used to read the direction. */
  previousScrollTop: number;
  /** Whether the follow was already released before this event. */
  wasReleased: boolean;
}

/** What the component stores after a scroll event. */
export interface ScrollFollowState {
  /** True when the follow is paused because the user moved away from the bottom. */
  userScrolledUp: boolean;
  /** True when the floating "jump to latest" affordance should be visible. */
  showScrollToBottom: boolean;
}

/** Distance from the bottom past which the follow stays released. */
export const FOLLOW_RELEASE_PX = 60;

/** Distance from the bottom past which the jump-to-latest button appears. */
export const JUMP_BUTTON_PX = 200;

/**
 * Upward movement, in pixels, below which a scroll is treated as noise rather
 * than intent. Sub-pixel offsets and elastic overscroll both land here.
 */
const UPWARD_EPSILON_PX = 1;

/** Remaining distance to the bottom of the container. */
export function distanceFromBottom(input: {
  scrollTop: number;
  scrollHeight: number;
  clientHeight: number;
}): number {
  return input.scrollHeight - input.scrollTop - input.clientHeight;
}

/** Resolve the follow state for one scroll event. */
export function computeScrollFollow(input: ScrollFollowInput): ScrollFollowState {
  const distance = distanceFromBottom(input);
  const movedUp = input.scrollTop < input.previousScrollTop - UPWARD_EPSILON_PX;

  // Near the bottom always means following, whoever put us there.
  const far = distance > FOLLOW_RELEASE_PX;
  const userScrolledUp = far && (movedUp || input.wasReleased);

  return {
    userScrolledUp,
    showScrollToBottom: userScrolledUp && distance > JUMP_BUTTON_PX,
  };
}

/**
 * The room to leave under the thread so the message just sent can sit at the
 * top of the viewport.
 *
 * A thread can only scroll as far as its content allows: right after a send,
 * what follows the sent message is shorter than the viewport, so its top cannot
 * reach the top. The missing height is added under the thread, and shrinks to
 * nothing as the answer fills it. With that room in place "the bottom" and
 * "the sent message at the top" are the same offset, which is what lets the
 * follow of an arriving answer take over without a jump.
 */
export function computeSendSpace(input: {
  /** Visible height of the container. */
  clientHeight: number;
  /** Offset of the top of the sent message inside the scrolled content. */
  anchorTop: number;
  /** Offset of the bottom of the last element of the thread. */
  contentBottom: number;
  /** Vertical padding of the container, top plus bottom. */
  inset: number;
}): number {
  const filled = input.contentBottom - input.anchorTop;
  return Math.max(0, Math.floor(input.clientHeight - input.inset - filled));
}

/** Where the thread goes when a message is sent. */
export type SendScroll = "sent-to-top" | "bottom";

/**
 * "Scroll the sent message to the top" needs a sent message to anchor on; with
 * the setting off, or nothing to anchor on, a send goes to the bottom as it
 * always did.
 */
export function sendScroll(input: { scrollSentToTop: boolean; hasAnchor: boolean }): SendScroll {
  return input.scrollSentToTop && input.hasAnchor ? "sent-to-top" : "bottom";
}

/**
 * Whether an arriving chunk moves the thread. It does not once the user took
 * the scroll over, nor with "follow the message being generated" off.
 */
export function followsGeneration(input: { followGeneration: boolean; released: boolean }): boolean {
  return input.followGeneration && !input.released;
}
