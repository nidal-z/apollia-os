/**
 * Operator-mode navigation (US-SP42-079).
 *
 * Grouped in two semantic sections so the sidebar reads as a task list,
 * not a catalogue of features:
 *   - Core          → what the operator *does*
 *   - Collaboration → how the operator *talks to agents and people*
 */
import type { ComponentType } from "svelte";
import type { Route } from "$lib/stores/navigation";
import {
  LayoutDashboard,
  Bot,
  FolderOpen,
  ListChecks,
  MessageSquare,
  Plug,
  ShieldCheck,
  Timer,
} from "lucide-svelte";

export interface NavEntry {
  route: Route;
  labelKey: string;
  icon: ComponentType;
}

export interface NavGroup {
  /** i18n key for the group header (e.g. `sidebar.group.core`). */
  labelKey: string;
  items: NavEntry[];
}

export const operatorGroups: NavGroup[] = [
  {
    labelKey: "sidebar.group.core",
    items: [
      { route: "dashboard", labelKey: "nav.home", icon: LayoutDashboard },
      { route: "agents", labelKey: "nav.my_assistants", icon: Bot },
      { route: "projects", labelKey: "nav.projects", icon: FolderOpen },
      { route: "tasks", labelKey: "nav.activity", icon: ListChecks },
      { route: "triggers", labelKey: "nav.my_triggers", icon: Timer },
    ],
  },
  {
    labelKey: "sidebar.group.collaboration",
    items: [
      { route: "chat", labelKey: "nav.chat", icon: MessageSquare },
      { route: "integrations", labelKey: "nav.connections", icon: Plug },
      { route: "inbox", labelKey: "nav.inbox_operator", icon: ShieldCheck },
    ],
  },
];
