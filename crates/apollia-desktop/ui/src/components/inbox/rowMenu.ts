/**
 * Builds the right-click / kebab menu for one inbox row.
 *
 * The item list is shared with the row's inline actions so a right-click
 * offers the same decisions as the expanded card, plus a builder-friendly
 * "Copy id". Pure: all effects are delegated to the passed handlers.
 */
import { Check, X, ShieldCheck, MessageSquare, Reply, Copy } from "lucide-svelte";
import type { ActionMenuItem } from "$lib/components/ui/action-menu";
import type { InboxItem } from "./types";
import { isApprovalItem, isChatToolItem, type Translate } from "./inboxModel";

export interface RowMenuHandlers {
  tr: Translate;
  /** Approve an approval item. */
  onApprove: (item: InboxItem) => void;
  /** Reject an approval item (opens the reason dialog). */
  onReject: (item: InboxItem) => void;
  /** Expand the row (used for "always allow" scope pick, reply, review). */
  onExpand: (item: InboxItem) => void;
  /** Jump to the source conversation. */
  onOpenChat: (sessionId: string) => void;
  /** Copy the item's runtime id to the clipboard. */
  onCopyId: (item: InboxItem) => void;
}

/** The clipboard-friendly runtime identifier for an item. */
export function itemId(item: InboxItem): string {
  switch (item.kind) {
    case "task":
      return item.source.task_id;
    case "ask_user":
      return item.source.request_id;
    case "plan":
      return item.source.planId;
    default:
      return item.source.sessionId;
  }
}

export function buildRowMenu(item: InboxItem, h: RowMenuHandlers): ActionMenuItem[] {
  const items: ActionMenuItem[] = [];
  const isApproval = isApprovalItem(item);

  if (isApproval && item.kind !== "plan") {
    items.push({
      id: "approve",
      label: h.tr("inbox.row.action_allow"),
      icon: Check,
      onclick: () => h.onApprove(item),
      testid: "inbox-menu-approve",
    });
    items.push({
      id: "reject",
      label: h.tr("inbox.refuse"),
      icon: X,
      variant: "destructive",
      onclick: () => h.onReject(item),
      testid: "inbox-menu-reject",
    });
  }

  if (isChatToolItem(item)) {
    items.push({
      id: "always",
      label: h.tr("inbox.always_accept"),
      icon: ShieldCheck,
      onclick: () => h.onExpand(item),
      testid: "inbox-menu-always",
    });
  }

  if (item.kind === "ask_user") {
    items.push({
      id: "reply",
      label: h.tr("inbox.row.action_reply"),
      icon: Reply,
      onclick: () => h.onExpand(item),
      testid: "inbox-menu-reply",
    });
  }

  if (item.kind === "plan") {
    items.push({
      id: "review",
      label: h.tr("inbox.row.action_review"),
      icon: Check,
      onclick: () => h.onExpand(item),
      testid: "inbox-menu-review",
    });
  }

  if (item.sessionId) {
    items.push({
      id: "open-chat",
      label: h.tr("inbox.row.open_conversation"),
      icon: MessageSquare,
      onclick: () => h.onOpenChat(item.sessionId as string),
      testid: "inbox-menu-open-chat",
    });
  }

  items.push({
    id: "copy-id",
    label: h.tr("inbox.row.copy_id"),
    icon: Copy,
    onclick: () => h.onCopyId(item),
    testid: "inbox-menu-copy-id",
  });

  return items;
}
