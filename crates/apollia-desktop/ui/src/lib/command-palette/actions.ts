/**
 * Command palette action catalogue.
 *
 * Builds the static list of navigation and command actions offered by
 * the palette. Actions carry an optional `developer` flag so the palette
 * hides technical actions while the developer mode is off, and an optional
 * `shortcut` rendered as kbd chips.
 *
 * Runtime-derived groups (Sessions, Settings sub-routes, Recently used)
 * live in `paletteIndex.ts` - this module is static data only.
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
  Database,
  Bell,
  Activity,
  Settings as SettingsIcon,
  Mic,
  Plus,
  Moon,
  Terminal,
  RefreshCcw,
  LogOut,
  PanelRightOpen,
  ToggleRight,
  Keyboard,
  Info,
  Landmark,
  Library,
} from "lucide-svelte";
// NOSONAR typescript:S1874 - Svelte 4 ComponentType retained for lucide-svelte compat
import type { ComponentType } from "svelte"; // NOSONAR typescript:S1874

/**
 * Local alias for Svelte 4 `ComponentType`. Centralizes deprecation
 * suppression - lucide-svelte's icon exports are typed against this alias.
 */
// NOSONAR typescript:S1874
type SvelteComponentType = ComponentType; // NOSONAR typescript:S1874
import { navigateTo, type Route } from "$lib/stores/navigation";
import { navigateToSettings } from "$lib/router";
import { toggleDeveloperMode } from "$lib/stores/developerMode";
import { themeMode } from "$lib/stores/theme";
import { companionStore } from "$lib/stores/companion";
import { onboardingModalOpen } from "$lib/stores/onboarding";
import { openNewChatRequested } from "$lib/stores/chat";

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
  /** Rendered label (translated - NOT an i18n key). */
  label: string;
  /** Translated keywords/synonyms boosting fuzzy search. */
  keywords?: string[];
  /** Optional secondary line shown under the label. */
  description?: string;
  /** Optional lucide icon component. */
  icon?: SvelteComponentType;
  /** Optional shortcut chips rendered at the row end. */
  shortcut?: string[];
  /** Group bucket. */
  kind: PaletteGroupKind;
  /** `true` for a technical action, shown only with the developer mode on. */
  developer?: boolean;
  /** `true` if this is a navigation action (hidden in actions-only mode). */
  navigation?: boolean;
  /** Invoked on Enter / click. */
  execute: () => void | Promise<void>;
}

// NOSONAR typescript:S1874 - navigator.platform retained for Tauri webview parity;
// userAgentData is not yet stable across all platforms Apollia targets.
const isMac =
  typeof navigator !== "undefined" && navigator.platform.includes("Mac"); // NOSONAR typescript:S1874
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
    icon: SvelteComponentType,
    keywords: string[] = [],
    developer?: boolean,
  ): PaletteAction => ({
    id,
    label: tr(labelKey),
    keywords: [...keywords, tr(labelKey).toLowerCase()],
    icon,
    kind: "pages",
    developer,
    navigation: true,
    execute: () => navigateTo(route),
  });

  return [
    // ── Pages ─────────────────────────────────────────────────────
    nav("nav.dashboard", "dashboard", "nav.dashboard", LayoutDashboard, ["home"]),
    nav("nav.agents", "agents", "nav.agents", Bot, ["assistants", "bots"]),
    nav("nav.tasks", "tasks", "nav.tasks", ListChecks, ["tasks", "activity"]),
    nav("nav.chat", "chat", "nav.chat", MessageSquare, ["chat", "conversation"]),
    nav("nav.inbox", "inbox", "nav.inbox", ShieldCheck, ["inbox", "approvals", "hitl"]),
    nav("nav.integrations", "integrations", "nav.connections", Plug, ["integrations", "mcp"]),
    nav("nav.llm", "llm", "nav.llm", Brain, ["llm", "models"]),
    nav("nav.library", "library", "nav.library", Library, ["library", "packages", "installed"]),
    nav("nav.automations", "automations", "nav.automations", Timer, ["automations", "triggers", "cron"]),
    nav("nav.memory", "memory", "nav.memory", Database, ["memory", "recall"], true),
    nav("nav.transcriptions", "transcriptions", "nav.transcriptions", Mic, ["stt", "speech"], true),
    nav("nav.observability", "observability", "nav.observability", Activity, ["observability", "metrics"], true),
    nav("nav.policy", "policy", "nav.policy", Landmark, ["policy", "politique", "organisation", "organization"]),
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
        navigateTo("chat");
        openNewChatRequested.set(Date.now());
      },
    },
    {
      id: "agents.create",
      label: tr("commandPalette.actions.createAgent"),
      keywords: ["agent", "create", "new"],
      icon: Bot,
      kind: "actions",
      execute: () => {
        navigateTo("agents");
        globalThis.dispatchEvent(new CustomEvent("apollia:agents:create"));
      },
    },
    {
      id: "automations.create",
      label: tr("commandPalette.actions.createTrigger"),
      keywords: ["automation", "trigger", "create", "cron", "schedule"],
      icon: Timer,
      kind: "actions",
      execute: () => {
        navigateTo("automations");
        globalThis.dispatchEvent(new CustomEvent("apollia:automations:create"));
      },
    },
    {
      id: "mode.toggle",
      label: tr("commandPalette.actions.toggleDeveloperMode"),
      keywords: ["developer", "toggle", "mode", "builder", "operator"],
      icon: ToggleRight,
      kind: "actions",
      execute: () => void toggleDeveloperMode(),
    },
    {
      id: "companion.toggle",
      label: tr("commandPalette.actions.toggleCompanion"),
      keywords: ["companion", "guide", "help", "aide", "panel", "toggle"],
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
        globalThis.dispatchEvent(new KeyboardEvent("keydown", { key: "?", bubbles: true })),
    },
    {
      id: "settings.shortcuts.open",
      label: tr("commandPalette.help.openShortcutsPage"),
      keywords: ["keyboard", "shortcuts", "page", "settings", "cheatsheet"],
      icon: Keyboard,
      kind: "settings",
      execute: () => {
        navigateToSettings("chat");
      },
    },
    {
      id: "settings.notifications.open",
      label: tr("settings.nav.notifications"),
      keywords: ["notifications", "channels", "alerts", "canaux", "alertes"],
      icon: Bell,
      kind: "settings",
      execute: () => {
        navigateToSettings("notifications");
      },
    },
    {
      id: "settings.danger.reset_onboarding",
      label: tr("commandPalette.danger.resetOnboarding"),
      keywords: ["reset", "onboarding", "danger", "welcome"],
      icon: RefreshCcw,
      kind: "settings",
      execute: () => {
        navigateToSettings("appearance");
      },
    },
    {
      id: "settings.danger.clear_memories",
      label: tr("commandPalette.danger.clearMemories"),
      keywords: ["clear", "delete", "memories", "danger"],
      icon: Brain,
      kind: "settings",
      execute: () => {
        navigateToSettings("general", "clear-memories");
      },
    },
    {
      id: "settings.danger.clear_logs",
      label: tr("commandPalette.danger.clearLogs"),
      keywords: ["clear", "delete", "logs", "danger"],
      icon: Terminal,
      kind: "settings",
      execute: () => {
        navigateToSettings("general", "clear-logs");
      },
    },
    {
      id: "settings.danger.factory_reset",
      label: tr("commandPalette.danger.factoryReset"),
      keywords: ["factory", "reset", "wipe", "danger"],
      icon: RefreshCcw,
      kind: "settings",
      execute: () => {
        navigateToSettings("general", "factory-reset");
      },
    },
    {
      id: "help.help",
      label: tr("commandPalette.help.help"),
      keywords: ["help", "aide", "guide", "getting started", "docs"],
      icon: Info,
      kind: "help",
      execute: () => {
        navigateToSettings("general");
      },
    },
    {
      id: "help.about",
      label: tr("commandPalette.help.about"),
      keywords: ["about", "apollia", "version"],
      icon: Info,
      kind: "help",
      execute: () => {
        navigateToSettings("general");
      },
    },
    // Removed: help.docs action (was navigating to obsolete onboarding route)
    {
      id: "help.onboarding_reset",
      label: tr("commandPalette.help.resetOnboarding"),
      keywords: ["onboarding", "reset", "restart"],
      icon: RefreshCcw,
      kind: "help",
      execute: () => {
        onboardingModalOpen.set(true);
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
        // Invoke the graceful quit command directly. A previous
        // `apollia:app:quit` CustomEvent had no listener, so quitting from the
        // palette was a silent no-op.
        void (async () => {
          try {
            const { invoke } = await import("@tauri-apps/api/core");
            await invoke("quit_app");
          } catch {
            // Web/dev fallback: no-op.
          }
        })();
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
              console.info("[apollia] dev console toggle requested");
            },
          },
        ]
      : []),
  ];
}
