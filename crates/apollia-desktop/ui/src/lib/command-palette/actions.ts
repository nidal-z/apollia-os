/**
 * Command palette action catalogue (US-SP42-078).
 *
 * Builds the static list of navigation and command actions offered by
 * the palette. Actions carry an optional `persona` tag so the palette
 * can hide builder-only actions from operator mode (and vice versa),
 * and an optional `shortcut` rendered as kbd chips.
 *
 * Runtime-derived groups (Sessions, Settings sub-routes, Recently used)
 * live in `paletteIndex.ts` — this module is static data only.
 */
import { get } from "svelte/store";
import { t } from "svelte-i18n";
import {
  LayoutDashboard,
  Bot,
  ListChecks,
  MessageSquare,
  ShieldCheck,
  Plug,
  Brain,
  Timer,
  GitBranch,
  Database,
  Bell,
  Activity,
  Settings as SettingsIcon,
  Mic,
  FolderOpen,
  LayoutGrid,
  Plus,
  Play,
  Square,
  Moon,
  Terminal,
  RefreshCcw,
  LogOut,
  PanelRightOpen,
  ToggleRight,
  Keyboard,
  Info,
  BookOpen,
} from "lucide-svelte";
import type { ComponentType } from "svelte";
import { navigateTo, type Route } from "$lib/stores/navigation";
import { uiMode, type UIMode } from "$lib/stores/mode";
import { themeMode } from "$lib/stores/theme";
import { companionStore } from "$lib/stores/companion";
import { onboardingStore } from "$lib/stores/onboarding";
import { openNewChatRequested } from "$lib/stores/chat";
import { openNewTaskRequested } from "$lib/stores/tasks";

export type PaletteGroupKind =
  | "pages"
  | "actions"
  | "sessions"
  | "settings"
  | "help"
  | "recent";

export interface PaletteAction {
  /** Stable id used for dedup, telemetry, and "Recently used" tracking. */
  id: string;
  /** Rendered label (translated — NOT an i18n key). */
  label: string;
  /** Translated keywords/synonyms boosting fuzzy search. */
  keywords?: string[];
  /** Optional secondary line shown under the label. */
  description?: string;
  /** Optional lucide icon component. */
  icon?: ComponentType;
  /** Optional shortcut chips rendered at the row end. */
  shortcut?: string[];
  /** Group bucket. */
  kind: PaletteGroupKind;
  /** Persona scoping. `undefined` → visible in both modes. */
  persona?: UIMode;
  /** `true` if this is a navigation action (hidden in actions-only mode). */
  navigation?: boolean;
  /** Invoked on Enter / click. */
  execute: () => void | Promise<void>;
}

const isMac = typeof navigator !== "undefined" && navigator.platform.includes("Mac");
const modKey = isMac ? "⌘" : "Ctrl";

function tr(key: string): string {
  try {
    return get(t)(key);
  } catch {
    return key;
  }
}

/** Build the static action catalogue. Call after i18n is ready. */
export function buildPaletteActions(): PaletteAction[] {
  const nav = (
    id: string,
    route: Route,
    labelKey: string,
    icon: ComponentType,
    keywords: string[] = [],
    persona?: UIMode,
  ): PaletteAction => ({
    id,
    label: tr(labelKey),
    keywords: [...keywords, tr(labelKey).toLowerCase()],
    icon,
    kind: "pages",
    persona,
    navigation: true,
    execute: () => navigateTo(route),
  });

  return [
    // ── Pages ─────────────────────────────────────────────────────
    nav("nav.dashboard", "dashboard", "nav.dashboard", LayoutDashboard, ["home"], "operator"),
    nav("nav.agents", "agents", "nav.agents", Bot, ["assistants", "bots"]),
    nav("nav.projects", "projects", "nav.projects", FolderOpen, ["project", "workspaces"]),
    nav("nav.tasks", "tasks", "nav.tasks", ListChecks, ["tasks", "activity"]),
    nav("nav.chat", "chat", "nav.chat", MessageSquare, ["chat", "conversation"]),
    nav("nav.approvals", "approvals", "nav.approvals", ShieldCheck, ["approvals", "hitl"], "operator"),
    nav("nav.inbox", "inbox", "nav.inbox_builder", ShieldCheck, ["inbox", "hitl"], "builder"),
    nav("nav.integrations", "integrations", "nav.connections", Plug, ["integrations", "mcp"]),
    nav("nav.llm", "llm", "nav.llm", Brain, ["llm", "models"], "builder"),
    nav("nav.triggers", "triggers", "nav.triggers", Timer, ["triggers", "cron"], "builder"),
    nav("nav.automations", "automations", "nav.automations", Timer, ["automations"], "operator"),
    nav("nav.pipelines", "pipelines", "nav.pipelines", GitBranch, ["pipelines", "dag"], "builder"),
    nav("nav.templates", "templates", "nav.templates", LayoutGrid, ["templates"]),
    nav("nav.memory", "memory", "nav.memory", Database, ["memory", "recall"], "builder"),
    nav("nav.transcriptions", "transcriptions", "nav.transcriptions", Mic, ["stt", "speech"], "builder"),
    nav("nav.notifications", "notifications", "nav.notifications", Bell, ["notifications"]),
    nav("nav.observability", "observability", "nav.observability", Activity, ["observability", "metrics"], "builder"),
    nav("nav.settings", "settings", "nav.settings", SettingsIcon, ["settings", "preferences"]),

    // ── Create / run actions ──────────────────────────────────────
    {
      id: "chat.new",
      label: tr("chat.new_chat"),
      keywords: ["chat", "new", "conversation", "nouveau"],
      icon: MessageSquare,
      shortcut: [modKey, "K"],
      kind: "actions",
      execute: () => {
        navigateTo("chat");
        openNewChatRequested.set(Date.now());
      },
    },
    {
      id: "tasks.new",
      label: tr("tasks.create_task_cta"),
      keywords: ["task", "new", "create", "tâche"],
      icon: Plus,
      shortcut: [modKey, "T"],
      kind: "actions",
      execute: () => {
        navigateTo("tasks");
        openNewTaskRequested.set(Date.now());
      },
    },
    {
      id: "agents.create",
      label: tr("commandPalette.actions.createAgent"),
      keywords: ["agent", "create", "new"],
      icon: Bot,
      kind: "actions",
      persona: "builder",
      execute: () => {
        navigateTo("agents");
        window.dispatchEvent(new CustomEvent("apollia:agents:create"));
      },
    },
    {
      id: "triggers.create",
      label: tr("commandPalette.actions.createTrigger"),
      keywords: ["trigger", "create", "cron", "schedule"],
      icon: Timer,
      kind: "actions",
      persona: "builder",
      execute: () => {
        navigateTo("triggers");
        window.dispatchEvent(new CustomEvent("apollia:triggers:create"));
      },
    },
    {
      id: "pipelines.run",
      label: tr("commandPalette.actions.runPipeline"),
      keywords: ["pipeline", "run", "execute"],
      icon: Play,
      kind: "actions",
      persona: "builder",
      execute: () => {
        navigateTo("pipelines");
        window.dispatchEvent(new CustomEvent("apollia:pipelines:run"));
      },
    },
    {
      id: "agents.start_all",
      label: tr("command.seed.agents_start_all"),
      keywords: ["start", "run", "all", "agents"],
      icon: Play,
      kind: "actions",
      persona: "builder",
      execute: () => window.dispatchEvent(new CustomEvent("apollia:agents:start-all")),
    },
    {
      id: "agents.stop_all",
      label: tr("command.seed.agents_stop_all"),
      keywords: ["stop", "halt", "all", "agents"],
      icon: Square,
      kind: "actions",
      persona: "builder",
      execute: () => window.dispatchEvent(new CustomEvent("apollia:agents:stop-all")),
    },
    {
      id: "mode.toggle",
      label: tr("commandPalette.actions.toggleMode"),
      keywords: ["operator", "builder", "toggle", "mode", "persona"],
      icon: ToggleRight,
      kind: "actions",
      execute: () => uiMode.update((m) => (m === "operator" ? "builder" : "operator")),
    },
    {
      id: "companion.toggle",
      label: tr("commandPalette.actions.toggleCompanion"),
      keywords: ["companion", "panel", "toggle"],
      icon: PanelRightOpen,
      kind: "actions",
      execute: () => companionStore.toggleCompanion(),
    },
    {
      id: "theme.toggle",
      label: tr("command.seed.toggle_dark"),
      keywords: ["dark", "light", "theme", "mode"],
      icon: Moon,
      kind: "actions",
      execute: () => themeMode.update((m) => (m === "dark" ? "light" : "dark")),
    },

    // ── Help ──────────────────────────────────────────────────────
    {
      id: "help.shortcuts",
      label: tr("commandPalette.help.shortcuts"),
      keywords: ["keyboard", "shortcuts", "help", "raccourcis"],
      icon: Keyboard,
      shortcut: ["?"],
      kind: "help",
      execute: () =>
        window.dispatchEvent(new KeyboardEvent("keydown", { key: "?", bubbles: true })),
    },
    {
      id: "help.about",
      label: tr("commandPalette.help.about"),
      keywords: ["about", "apollia", "version"],
      icon: Info,
      kind: "help",
      execute: () => {
        navigateTo("settings");
        window.dispatchEvent(new CustomEvent("apollia:settings:goto", { detail: "system" }));
      },
    },
    {
      id: "help.docs",
      label: tr("commandPalette.help.docs"),
      keywords: ["docs", "documentation", "book"],
      icon: BookOpen,
      kind: "help",
      execute: () => navigateTo("onboarding"),
    },
    {
      id: "help.onboarding_reset",
      label: tr("commandPalette.help.resetOnboarding"),
      keywords: ["onboarding", "reset", "restart"],
      icon: RefreshCcw,
      kind: "help",
      execute: () => {
        void onboardingStore.advancePhase("welcome").catch(() => {});
      },
    },
    {
      id: "app.quit",
      label: tr("commandPalette.help.quit"),
      keywords: ["quit", "exit", "close"],
      icon: LogOut,
      shortcut: [modKey, "Q"],
      kind: "help",
      execute: () => {
        window.dispatchEvent(new CustomEvent("apollia:app:quit"));
      },
    },

    // ── Dev-only ──────────────────────────────────────────────────
    ...((import.meta as ImportMeta & { env?: { DEV?: boolean } }).env?.DEV
      ? [
          {
            id: "dev.console",
            label: tr("commandPalette.help.toggleDevConsole"),
            keywords: ["dev", "console", "debug"],
            icon: Terminal,
            kind: "help" as const,
            execute: () => {
              // eslint-disable-next-line no-console
              console.info("[apollia] dev console toggle requested");
            },
          },
        ]
      : []),
  ];
}
