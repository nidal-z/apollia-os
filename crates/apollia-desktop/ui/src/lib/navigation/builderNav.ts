/**
 * Builder-mode navigation (US-SP42-079).
 *
 * Extracted from the Sidebar inline definition so both modes share the
 * same `NavGroup` shape (see `operatorNav.ts`).
 */
import type { NavGroup } from "./operatorNav";
import {
  LayoutDashboard,
  Bot,
  FolderOpen,
  ListChecks,
  MessageSquare,
  ShieldCheck,
  Brain,
  Timer,
  GitBranch,
  Plug,
  LayoutGrid,
  Database,
  Mic,
  Bell,
  Activity,
} from "lucide-svelte";

export const builderGroups: NavGroup[] = [
  {
    labelKey: "nav.operations",
    items: [
      { route: "dashboard", labelKey: "nav.dashboard", icon: LayoutDashboard },
      { route: "agents", labelKey: "nav.agents", icon: Bot },
      { route: "projects", labelKey: "nav.projects", icon: FolderOpen },
      { route: "tasks", labelKey: "nav.tasks", icon: ListChecks },
      { route: "chat", labelKey: "nav.chat", icon: MessageSquare },
      { route: "inbox", labelKey: "nav.inbox_builder", icon: ShieldCheck },
    ],
  },
  {
    labelKey: "nav.infrastructure",
    items: [
      { route: "llm", labelKey: "nav.llm", icon: Brain },
      { route: "triggers", labelKey: "nav.triggers", icon: Timer },
      { route: "pipelines", labelKey: "nav.pipelines", icon: GitBranch },
      { route: "integrations", labelKey: "nav.mcp_servers", icon: Plug },
      { route: "templates", labelKey: "nav.templates", icon: LayoutGrid },
    ],
  },
  {
    labelKey: "nav.data",
    items: [
      { route: "memory", labelKey: "nav.memory", icon: Database },
      { route: "transcriptions", labelKey: "nav.transcriptions", icon: Mic },
      { route: "notifications", labelKey: "nav.notifications", icon: Bell },
      { route: "observability", labelKey: "nav.observability", icon: Activity },
    ],
  },
];
