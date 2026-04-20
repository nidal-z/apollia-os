/**
 * Runtime index for the command palette (US-SP42-078).
 *
 * Builds the groups that feed the `<CommandPalette>` component:
 *   - Recently used  (MRU, when the input is empty)
 *   - Pages          (navigation routes, persona-filtered)
 *   - Actions        (commands that do something, persona-filtered)
 *   - Sessions       (10 most recent chats)
 *   - Settings       (9 sub-routes under /settings)
 *   - Help           (shortcuts, about, docs, onboarding reset, quit)
 *
 * The result is a derived `paletteGroups` store, re-computed whenever
 * the underlying sources (actions, sessions, recency, persona) change.
 */
import { derived, writable, type Readable } from "svelte/store";
import { t } from "svelte-i18n";
import { get } from "svelte/store";
import { MessageSquare, Settings as SettingsIcon } from "lucide-svelte";
import type { CommandPaletteGroup } from "$lib/components/ui/command";
import { commands, type CommandItem } from "$lib/stores/commandPalette";
import { uiMode, type UIMode } from "$lib/stores/mode";
import { recentActionIds, touchRecentAction } from "$lib/stores/recentActionsStore";
import { recentSessions, touchRecentSession } from "$lib/stores/recentSessionsStore";
import { navigateTo } from "$lib/stores/navigation";
import {
  SETTINGS_SUB_ROUTES,
  goToSettingsSubRoute,
  type SettingsSubRoute,
} from "$lib/stores/settings";
import type { PaletteAction } from "./actions";

/** Palette display mode — `actions` hides navigation entries (Cmd+Shift+P). */
export type PaletteMode = "all" | "actions";

export const paletteMode = writable<PaletteMode>("all");

const _actions = writable<PaletteAction[]>([]);

/** Registered action catalogue; set once at startup by the orchestrator. */
export function setPaletteActions(list: PaletteAction[]): void {
  _actions.set(list);
}

function tr(key: string): string {
  try {
    return get(t)(key);
  } catch {
    return key;
  }
}

function settingsSubRouteLabel(route: SettingsSubRoute): string {
  return tr(`settings.nav.${route}`);
}

function actionToCommandItem(action: PaletteAction, group: string): CommandItem {
  return {
    id: action.id,
    label: action.label,
    description: action.description,
    icon: action.icon,
    shortcut: action.shortcut,
    group,
    keywords: action.keywords,
    action: () => {
      touchRecentAction(action.id);
      return action.execute();
    },
  };
}

function filterByPersona<T extends { persona?: UIMode }>(list: T[], mode: UIMode): T[] {
  return list.filter((x) => !x.persona || x.persona === mode);
}

/** Groups shown by the palette — reactive to stores + persona + mode. */
export const paletteGroups: Readable<CommandPaletteGroup[]> = derived(
  [
    _actions,
    uiMode,
    paletteMode,
    recentActionIds,
    recentSessions,
    commands,
  ],
  ([$actions, $mode, $paletteMode, $recentIds, $recentSessions, $extra]) => {
    const groups: CommandPaletteGroup[] = [];
    const actionsOnly = $paletteMode === "actions";

    // Persona-filtered catalogue.
    const visible = filterByPersona($actions, $mode);

    // ── Recently used (only when palette first opens / empty input) ─
    if ($recentIds.length > 0) {
      const byId = new Map(visible.map((a) => [a.id, a]));
      const recent: CommandItem[] = [];
      for (const id of $recentIds.slice(0, 5)) {
        const a = byId.get(id);
        if (!a) continue;
        if (actionsOnly && a.navigation) continue;
        recent.push(actionToCommandItem(a, tr("commandPalette.groups.recent")));
      }
      if (recent.length > 0) {
        groups.push({ label: tr("commandPalette.groups.recent"), items: recent });
      }
    }

    // ── Pages ────────────────────────────────────────────────────────
    if (!actionsOnly) {
      const pages = visible
        .filter((a) => a.kind === "pages")
        .map((a) => actionToCommandItem(a, tr("commandPalette.groups.pages")));
      if (pages.length > 0) {
        groups.push({ label: tr("commandPalette.groups.pages"), items: pages });
      }
    }

    // ── Actions ──────────────────────────────────────────────────────
    const actions = visible
      .filter((a) => a.kind === "actions")
      .map((a) => actionToCommandItem(a, tr("commandPalette.groups.actions")));
    if (actions.length > 0) {
      groups.push({ label: tr("commandPalette.groups.actions"), items: actions });
    }

    // ── Sessions (chat) ──────────────────────────────────────────────
    if (!actionsOnly && $recentSessions.length > 0) {
      const sessionItems: CommandItem[] = $recentSessions.slice(0, 10).map((s) => ({
        id: `session.${s.id}`,
        label: s.title?.trim() || tr("chat.untitled_session"),
        description: s.last_message_preview ?? undefined,
        icon: MessageSquare,
        group: tr("commandPalette.groups.sessions"),
        keywords: [s.agent_name ?? "", s.id].filter(Boolean),
        action: () => {
          touchRecentSession(s.id);
          navigateTo("chat");
          window.dispatchEvent(
            new CustomEvent("apollia:chat:open-session", { detail: { id: s.id } }),
          );
        },
      }));
      groups.push({ label: tr("commandPalette.groups.sessions"), items: sessionItems });
    }

    // ── Settings sub-routes ──────────────────────────────────────────
    if (!actionsOnly) {
      const settings: CommandItem[] = SETTINGS_SUB_ROUTES.map((sub) => ({
        id: `settings.${sub}`,
        label: `${tr("nav.settings")} → ${settingsSubRouteLabel(sub)}`,
        icon: SettingsIcon,
        group: tr("commandPalette.groups.settings"),
        keywords: ["settings", "preferences", sub],
        action: () => {
          touchRecentAction(`settings.${sub}`);
          navigateTo("settings");
          goToSettingsSubRoute(sub);
        },
      }));
      groups.push({ label: tr("commandPalette.groups.settings"), items: settings });
    }

    // ── Plugin-style extras registered via `registerCommand` (e.g. the
    // chat route contributes recent sessions / templates / slash commands
    // while it is mounted). Buckets are keyed by the entry's `group` label.
    if ($extra.length > 0) {
      const bucketed = new Map<string, CommandItem[]>();
      for (const cmd of $extra) {
        const list = bucketed.get(cmd.group) ?? [];
        list.push(cmd);
        bucketed.set(cmd.group, list);
      }
      for (const [label, items] of bucketed) {
        groups.push({ label, items });
      }
    }

    // ── Help ─────────────────────────────────────────────────────────
    const help = visible
      .filter((a) => a.kind === "help")
      .map((a) => actionToCommandItem(a, tr("commandPalette.groups.help")));
    if (help.length > 0) {
      groups.push({ label: tr("commandPalette.groups.help"), items: help });
    }

    return groups;
  },
);
