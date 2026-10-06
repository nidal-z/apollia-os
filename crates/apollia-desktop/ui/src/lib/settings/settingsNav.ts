/**
 * The settings navigation column: two groups, ranked by use rather than by
 * architecture. "Settings" holds what a user adjusts, "System" what the
 * machine runs on. Integrations holds the defaults Apollia grants (the
 * accesses of a new agent, the profile in a new conversation) and points to
 * the Authorizations and Connections screens; it is not the former OAuth
 * section. Notifications comes just before Developer, and Developer
 * closes the use-ranked part of its group: it is the switch a business user
 * never has to touch. Configuration, the editor of `apollia.toml`, follows it
 * and is listed only with the developer mode on. The sections that leave the
 * settings for their own screens (LLM backends, Model Hub) follow it until
 * they move. Three already left: the OAuth integrations live on the
 * Connections screen, on the page of each connection; the built-in tools are
 * a source of tools on that same screen; the permission rules live on the
 * Authorizations screen.
 */
import {
  Settings2,
  Palette,
  User,
  MessageSquare,
  Mic,
  SlidersHorizontal,
  Plug,
  Code,
  Bell,
  Activity,
  HardDrive,
} from "lucide-svelte";
import type { SettingsSubRoute } from "$lib/stores/settings";

export type SettingsNavEntry = {
  key: SettingsSubRoute;
  labelKey: string;
  icon: typeof Palette;
  /** Id of the catalogue shortcut that opens this section, shown in the column. */
  shortcutId?: string;
  /** Listed only with the developer mode on. */
  developerOnly?: boolean;
};

export type SettingsNavCluster = {
  labelKey: string;
  entries: SettingsNavEntry[];
};

export const SETTINGS_NAV: SettingsNavCluster[] = [
  {
    labelKey: "settings.nav.cluster_settings",
    entries: [
      { key: "general", labelKey: "settings.nav.general", icon: Settings2 },
      { key: "appearance", labelKey: "settings.nav.appearance", icon: Palette },
      { key: "profile", labelKey: "settings.nav.profile", icon: User },
      { key: "chat", labelKey: "settings.nav.chat", icon: MessageSquare },
      { key: "stt", labelKey: "settings.nav.stt", icon: Mic },
      { key: "model-defaults", labelKey: "settings.nav.model_defaults", icon: SlidersHorizontal },
      { key: "integrations", labelKey: "settings.nav.integrations", icon: Plug },
      { key: "notifications", labelKey: "settings.nav.notifications", icon: Bell },
      { key: "developer", labelKey: "settings.nav.developer", icon: Code },
      {
        key: "configuration",
        labelKey: "settings.nav.configuration",
        icon: Code,
        developerOnly: true,
      },
    ],
  },
  {
    labelKey: "settings.nav.cluster_system",
    entries: [
      {
        key: "engines",
        labelKey: "settings.nav.engines",
        icon: Activity,
        shortcutId: "settings.engines",
      },
      {
        key: "hardware",
        labelKey: "settings.nav.hardware",
        icon: HardDrive,
        shortcutId: "settings.hardware",
      },
    ],
  },
];

/** The column as shown with the developer mode in this position. */
export function visibleSettingsNav(developerMode: boolean): SettingsNavCluster[] {
  return SETTINGS_NAV.map((cluster) => ({
    ...cluster,
    entries: cluster.entries.filter((entry) => developerMode || !entry.developerOnly),
  }));
}
