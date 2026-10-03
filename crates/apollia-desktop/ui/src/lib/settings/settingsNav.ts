/**
 * The settings navigation column: two groups, ranked by use rather than by
 * architecture. "Settings" holds what a user adjusts, "System" what the
 * machine runs on. Developer comes last of its group: it is the switch a
 * business user never has to touch. The sections that leave the settings for
 * their own screens (LLM backends, Model Hub, Tools, Permissions, OAuth
 * integrations) follow it until they move.
 */
import {
  Settings2,
  Palette,
  User,
  MessageSquare,
  Mic,
  SlidersHorizontal,
  Code,
  Activity,
  HardDrive,
  Cpu,
  Download,
  Wrench,
  ShieldCheck,
  PlugZap,
} from "lucide-svelte";
import type { SettingsSubRoute } from "$lib/stores/settings";

export type SettingsNavEntry = {
  key: SettingsSubRoute;
  labelKey: string;
  icon: typeof Palette;
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
      { key: "developer", labelKey: "settings.nav.developer", icon: Code },
      { key: "llm", labelKey: "settings.nav.llm", icon: Cpu },
      { key: "model-hub", labelKey: "settings.nav.model_hub", icon: Download },
      { key: "tools", labelKey: "settings.nav.tools", icon: Wrench },
      { key: "permissions", labelKey: "settings.nav.permissions", icon: ShieldCheck },
      { key: "integrations", labelKey: "settings.nav.integrations", icon: PlugZap },
    ],
  },
  {
    labelKey: "settings.nav.cluster_system",
    entries: [
      { key: "engines", labelKey: "settings.nav.engines", icon: Activity },
      { key: "hardware", labelKey: "settings.nav.hardware", icon: HardDrive },
    ],
  },
];
