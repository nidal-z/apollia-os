/**
 * The follow-the-answer scroll of a conversation.
 *
 * `computeScrollFollow` decides whether the operator has taken the scroll over;
 * this factory carries that decision as live state, coalesces several scroll
 * requests inside one animation frame, and counts what landed while the
 * operator was reading further up.
 */
import { computeScrollFollow } from "$lib/chat/scrollFollow";

export interface ConversationScroll {
  /** Bound to the scrolling element by the view. */
  container: HTMLDivElement | undefined;
  /** True once the operator scrolled away from the bottom. */
  readonly released: boolean;
  /** True when the jump-to-latest button should show. */
  readonly showJump: boolean;
  /** Messages that landed while the operator was reading further up. */
  readonly unread: number;
  /**
   * Scroll to the newest content. `force` also overrides a released scroll,
   * which is what the operator asked for by pressing the button.
   */
  toBottom(force?: boolean): void;
  /** The `onscroll` handler of the container. */
  onScroll(): void;
  /** The jump-to-latest button. */
  jumpToLatest(): void;
  /** Count what landed while released; call whenever the message count moves. */
  noteMessageCount(count: number): void;
  /** Forget everything, e.g. when the conversation is switched. */
  reset(): void;
  /** Cancel a pending follow frame on destroy. */
  dispose(): void;
}

/**
 * `isBusy` tells a turn still arriving from a settled thread: a smooth
 * animation retargeted on every chunk never reaches the bottom, and its lag is
 * indistinguishable from content the operator has not read.
 */
export function createConversationScroll(isBusy: () => boolean): ConversationScroll {
  let container = $state<HTMLDivElement | undefined>(undefined);
  let released = $state(false);
  let showJump = $state(false);
  let unread = $state(0);
  /** Pending follow frame, so several calls within one frame scroll once. */
  let followFrame: number | null = null;
  /** Offset at the previous scroll event, read to tell a follow from a user. */
  let previousScrollTop = 0;
  let lastSeenMessageCount = 0;

  function toBottom(force = false): void {
    if (!force && released) return;
    if (followFrame !== null) return;
    followFrame = requestAnimationFrame(() => {
      followFrame = null;
      if (!container) return;
      const behavior: ScrollBehavior = force || isBusy() ? "instant" : "smooth";
      container.scrollTo({ top: container.scrollHeight, behavior });
    });
  }

  return {
    get container() {
      return container;
    },
    set container(el: HTMLDivElement | undefined) {
      container = el;
    },
    get released() {
      return released;
    },
    get showJump() {
      return showJump;
    },
    get unread() {
      return unread;
    },
    toBottom,

    onScroll(): void {
      if (!container) return;
      const { scrollTop, scrollHeight, clientHeight } = container;
      const next = computeScrollFollow({
        scrollTop,
        scrollHeight,
        clientHeight,
        previousScrollTop,
        wasReleased: released,
      });
      previousScrollTop = scrollTop;
      released = next.userScrolledUp;
      showJump = next.showScrollToBottom;
      if (!released) unread = 0;
    },

    jumpToLatest(): void {
      released = false;
      unread = 0;
      showJump = false;
      toBottom(true);
    },

    noteMessageCount(count: number): void {
      if (released && count > lastSeenMessageCount) {
        unread += count - lastSeenMessageCount;
      }
      lastSeenMessageCount = count;
    },

    reset(): void {
      released = false;
      showJump = false;
      unread = 0;
      previousScrollTop = 0;
      lastSeenMessageCount = 0;
    },

    dispose(): void {
      if (followFrame !== null) {
        cancelAnimationFrame(followFrame);
        followFrame = null;
      }
    },
  };
}
