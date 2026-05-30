/**
 * Runtime index for the command palette.
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
import { pendingChatSessionId } from "$lib/stores/chat";
import {
  SETTINGS_SUB_ROUTES,
  goToSettingsSubRoute,
  type SettingsSubRoute,
} from "$lib/stores/settings";
import type { PaletteAction } from "./actions";

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

function buildRecentGroup(
  visible: PaletteAction[],
  recentIds: string[],
): CommandPaletteGroup | null {
  if (recentIds.length === 0) return null;
  const byId = new Map(visible.map((a) => [a.id, a]));
  const label = tr("commandPalette.groups.recent");
  const recent: CommandItem[] = [];
  for (const id of recentIds.slice(0, 5)) {
    const a = byId.get(id);
    if (a) recent.push(actionToCommandItem(a, label));
  }
  return recent.length > 0 ? { label, items: recent } : null;
}

function buildKindGroup(
  visible: PaletteAction[],
  kind: PaletteAction["kind"],
  label: string,
): CommandPaletteGroup | null {
  const items = visible
    .filter((a) => a.kind === kind)
    .map((a) => actionToCommandItem(a, label));
  return items.length > 0 ? { label, items } : null;
}

function buildSessionsGroup(
  recentSessions: ReadonlyArray<{
    id: string;
    title: string | null;
    last_message_preview: string | null;
    agent_name: string | null;
  }>,
): CommandPaletteGroup | null {
  if (recentSessions.length === 0) return null;
  const label = tr("commandPalette.groups.sessions");
  const items: CommandItem[] = recentSessions.slice(0, 10).map((s) => ({
    id: `session.${s.id}`,
    label: s.title?.trim() || tr("chat.untitled_session"),
    description: s.last_message_preview ?? undefined,
    icon: MessageSquare,
    group: label,
    keywords: [s.agent_name ?? "", s.id].filter(Boolean),
    action: () => {
      touchRecentSession(s.id);
      // Set the deep-link store BEFORE navigation so Chat.svelte's
      // onMount subscription picks the right session immediately.
      pendingChatSessionId.set(s.id);
      navigateTo("chat");
    },
  }));
  return { label, items };
}

function buildSettingsGroup(): CommandPaletteGroup {
  const label = tr("commandPalette.groups.settings");
  const items: CommandItem[] = SETTINGS_SUB_ROUTES.map((sub) => ({
    id: `settings.${sub}`,
    label: `${tr("nav.settings")} → ${settingsSubRouteLabel(sub)}`,
    icon: SettingsIcon,
    group: label,
    keywords: ["settings", "preferences", sub],
    action: () => {
      touchRecentAction(`settings.${sub}`);
      navigateTo("settings");
      goToSettingsSubRoute(sub);
    },
  }));
  return { label, items };
}

function buildExtraGroups(extras: CommandItem[]): CommandPaletteGroup[] {
  if (extras.length === 0) return [];
  const bucketed = new Map<string, CommandItem[]>();
  for (const cmd of extras) {
    const list = bucketed.get(cmd.group) ?? [];
    list.push(cmd);
    bucketed.set(cmd.group, list);
  }
  const groups: CommandPaletteGroup[] = [];
  for (const [label, items] of bucketed) {
    groups.push({ label, items });
  }
  return groups;
}

/** Groups shown by the palette — reactive to stores + persona. */
export const paletteGroups: Readable<CommandPaletteGroup[]> = derived(
  [
    _actions,
    uiMode,
    recentActionIds,
    recentSessions,
    commands,
  ],
  ([$actions, $mode, $recentIds, $recentSessions, $extra]) => {
    // Persona-filtered catalogue.
    const visible = filterByPersona($actions, $mode);

    // Build the ordered sequence of candidate groups in one expression, then
    // filter out absent (null) buckets. Avoids multiple chained `push` calls.
    // ── Plugin-style extras registered via `registerCommand` (e.g. the
    // chat route contributes recent sessions / templates / slash commands
    // while it is mounted). Buckets are keyed by the entry's `group` label.
    const candidates: Array<CommandPaletteGroup | null> = [
      buildRecentGroup(visible, $recentIds),
      buildKindGroup(visible, "pages", tr("commandPalette.groups.pages")),
      buildKindGroup(visible, "actions", tr("commandPalette.groups.actions")),
      buildSessionsGroup($recentSessions),
      buildSettingsGroup(),
      ...buildExtraGroups($extra),
      buildKindGroup(visible, "help", tr("commandPalette.groups.help")),
    ];
    const groups: CommandPaletteGroup[] = candidates.filter(
      (g): g is CommandPaletteGroup => g !== null,
    );

    return groups;
  },
);
