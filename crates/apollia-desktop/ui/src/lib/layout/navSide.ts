/**
 * Which side each column of the shell sits on, from the navigation position
 * chosen in Settings > Appearance.
 *
 * The navigation bar and the detail sidebar are always on opposite sides: the
 * bar on the right sends the detail sidebar to the left. Every class that
 * depends on the side is decided here, so the shell, the rail, the drawer and
 * the detail sidebar cannot disagree.
 */
import type { NavPosition } from "$lib/stores/chatAppearance";

export interface NavSide {
  /** Side of the navigation bar. */
  nav: NavPosition;
  /** Side of the detail sidebar, always the other one. */
  detail: NavPosition;
  /** Row direction of the shell (navigation bar, then the application column). */
  shellRow: string;
  /** Row direction of the content (screen, then detail sidebar). */
  contentRow: string;
  /** Border of the rail on its inner edge. */
  railBorder: string;
  /** Where a rail tooltip opens, toward the content. */
  railTooltip: string;
  /** Anchor and inner border of the drawer the rail becomes on a narrow window. */
  drawerEdge: string;
  /** Horizontal offset the drawer flies in from, in pixels. */
  drawerFlyX: number;
  /** Border of the detail sidebar on its inner edge. */
  detailBorder: string;
}

const DRAWER_FLY_PX = 280;

export function navSide(position: NavPosition): NavSide {
  if (position === "right") {
    return {
      nav: "right",
      detail: "left",
      shellRow: "flex-row-reverse",
      contentRow: "flex-row-reverse",
      railBorder: "border-l",
      railTooltip: "right-full mr-2",
      drawerEdge: "right-0 border-l",
      drawerFlyX: DRAWER_FLY_PX,
      detailBorder: "border-r",
    };
  }
  return {
    nav: "left",
    detail: "right",
    shellRow: "flex-row",
    contentRow: "flex-row",
    railBorder: "border-r",
    railTooltip: "left-full ml-2",
    drawerEdge: "left-0 border-r",
    drawerFlyX: -DRAWER_FLY_PX,
    detailBorder: "border-l",
  };
}
