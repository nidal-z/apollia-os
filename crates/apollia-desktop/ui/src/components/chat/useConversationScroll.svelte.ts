/**
 * The follow-the-answer scroll of a conversation.
 *
 * `computeScrollFollow` decides whether the operator has taken the scroll over;
 * this factory carries that decision as live state, coalesces several scroll
 * requests inside one animation frame, and counts what landed while the
 * operator was reading further up.
 *
 * Two choices of Settings > Appearance are read here, at the moment they
 * apply: whether a sent message scrolls to the top of the thread, and whether
 * the thread follows an answer while it arrives.
 */
import { get } from "svelte/store";
import {
  computeScrollFollow,
  computeSendSpace,
  followsGeneration,
  sendScroll,
} from "$lib/chat/scrollFollow";
import { chatAppearance } from "$lib/stores/chatAppearance";

export interface ConversationScroll {
  /** Bound to the scrolling element by the view. */
  container: HTMLDivElement | undefined;
  /** True once the operator scrolled away from the bottom. */
  readonly released: boolean;
  /** True when the jump-to-latest button should show. */
  readonly showJump: boolean;
  /** Messages that landed while the operator was reading further up. */
  readonly unread: number;
  /** Height, in pixels, of the room kept under the thread after a send. */
  readonly sendSpace: number;
  /**
   * A message was just sent and is in the thread: bring it to the top, or go
   * to the bottom, as Appearance says.
   */
  sent(): void;
  /**
   * A chunk of the answer landed, or the turn settled: follow it, unless
   * Appearance says not to.
   */
  follow(): void;
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
  /** Top of the last sent message inside the scrolled content, once pinned. */
  let anchorTop: number | null = null;
  let sendSpace = $state(0);

  function padding(el: HTMLElement): { top: number; bottom: number } {
    const style = getComputedStyle(el);
    return {
      top: parseFloat(style.paddingTop) || 0,
      bottom: parseFloat(style.paddingBottom) || 0,
    };
  }

  /** Offset of a child edge inside the scrolled content. */
  function offsetIn(el: HTMLElement, edge: "top" | "bottom", child: Element): number {
    return child.getBoundingClientRect()[edge] - el.getBoundingClientRect().top + el.scrollTop;
  }

  /** Bottom of the thread proper, the spacer left out. */
  function contentBottom(el: HTMLElement): number {
    const spacer = el.querySelector("[data-chat-send-spacer]");
    const last = spacer?.previousElementSibling;
    return last ? offsetIn(el, "bottom", last) : 0;
  }

  function refreshSendSpace(): void {
    if (!container || anchorTop === null) {
      sendSpace = 0;
      return;
    }
    const pad = padding(container);
    sendSpace = computeSendSpace({
      clientHeight: container.clientHeight,
      anchorTop,
      contentBottom: contentBottom(container),
      inset: pad.top + pad.bottom,
    });
  }

  function dropAnchor(): void {
    anchorTop = null;
    sendSpace = 0;
  }

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
    get sendSpace() {
      return sendSpace;
    },

    // A forced scroll from outside is a load, a turn played again or the
    // operator's own jump: the thread it pinned a message in is not this one.
    toBottom(force = false): void {
      if (force) dropAnchor();
      toBottom(force);
    },

    sent(): void {
      released = false;
      showJump = false;
      unread = 0;
      const groups = container?.querySelectorAll("[data-message-id]");
      const anchor = groups && groups.length > 0 ? groups[groups.length - 1] : null;
      const mode = sendScroll({
        scrollSentToTop: get(chatAppearance).scrollSentToTop,
        hasAnchor: anchor !== null,
      });
      if (mode === "bottom" || !container || !anchor) {
        dropAnchor();
        toBottom(true);
        return;
      }
      const top = offsetIn(container, "top", anchor);
      anchorTop = top;
      refreshSendSpace();
      // The room is laid out before the next frame, which is when the offset
      // becomes reachable.
      if (followFrame !== null) cancelAnimationFrame(followFrame);
      followFrame = requestAnimationFrame(() => {
        followFrame = null;
        if (!container) return;
        container.scrollTo({ top: top - padding(container).top, behavior: "instant" });
      });
    },

    follow(): void {
      // The room only ever shrinks while an answer arrives; once it is gone
      // there is nothing left to measure on each chunk.
      if (sendSpace > 0) refreshSendSpace();
      if (!followsGeneration({ followGeneration: get(chatAppearance).followGeneration, released })) {
        return;
      }
      toBottom();
    },

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
      refreshSendSpace();
    },

    reset(): void {
      released = false;
      showJump = false;
      unread = 0;
      previousScrollTop = 0;
      lastSeenMessageCount = 0;
      dropAnchor();
    },

    dispose(): void {
      if (followFrame !== null) {
        cancelAnimationFrame(followFrame);
        followFrame = null;
      }
    },
  };
}
