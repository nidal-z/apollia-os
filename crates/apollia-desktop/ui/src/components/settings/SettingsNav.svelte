<script lang="ts">
  import { t } from "svelte-i18n";
  import {
    Palette,
    User,
    Mic,
    Cpu,
    Sliders,
    Info,
    Brain,
    Keyboard,
    AlertTriangle,
    Download,
  } from "lucide-svelte";
  import type { SettingsSubRoute } from "$lib/stores/settings";

  interface Props {
    active: SettingsSubRoute;
    onnavigate: (route: SettingsSubRoute) => void;
  }

  let { active, onnavigate }: Props = $props();

  type Entry = {
    key: SettingsSubRoute;
    labelKey: string;
    icon: typeof Palette;
  };

  type Cluster = {
    labelKey: string;
    entries: Entry[];
    tone?: "default" | "danger";
  };

  const CLUSTERS: Cluster[] = [
    {
      labelKey: "settings.nav.cluster_personalization",
      entries: [
        { key: "appearance", labelKey: "settings.nav.appearance", icon: Palette },
        { key: "profile", labelKey: "settings.nav.profile", icon: User },
      ],
    },
    {
      labelKey: "settings.nav.cluster_ai",
      entries: [
        { key: "stt", labelKey: "settings.nav.stt", icon: Mic },
        { key: "llm", labelKey: "settings.nav.llm", icon: Cpu },
        { key: "model-hub", labelKey: "settings.nav.model_hub", icon: Download },
      ],
    },
    {
      labelKey: "settings.nav.cluster_system",
      entries: [
        { key: "configuration", labelKey: "settings.nav.configuration", icon: Sliders },
        { key: "system", labelKey: "settings.nav.system", icon: Info },
        { key: "memories", labelKey: "settings.nav.memories", icon: Brain },
        { key: "shortcuts", labelKey: "settings.nav.shortcuts", icon: Keyboard },
      ],
    },
    {
      labelKey: "settings.nav.cluster_danger",
      tone: "danger",
      entries: [
        { key: "danger", labelKey: "settings.nav.danger", icon: AlertTriangle },
      ],
    },
  ];
</script>

<nav
  class="sticky top-4 flex w-56 flex-shrink-0 flex-col gap-6"
  aria-label={$t("settings.nav.aria_label")}
  data-testid="settings-nav"
>
  {#each CLUSTERS as cluster (cluster.labelKey)}
    <div class="space-y-1.5">
      <h3
        class="px-2 text-xs uppercase tracking-wide {cluster.tone === 'danger'
          ? 'text-destructive/80'
          : 'text-muted-foreground'}"
      >
        {$t(cluster.labelKey)}
      </h3>
      <ul class="space-y-0.5">
        {#each cluster.entries as entry (entry.key)}
          {@const SvelteIcon = entry.icon}
          {@const isActive = active === entry.key}
          {@const isDanger = cluster.tone === "danger"}
          <li>
            <button
              type="button"
              onclick={() => onnavigate(entry.key)}
              aria-current={isActive ? "page" : undefined}
              data-settings-route={entry.key}
              data-testid="settings-nav-{entry.key}"
              class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-sm transition-colors
                {isActive
                  ? isDanger
                    ? 'bg-destructive/10 text-destructive font-medium'
                    : 'bg-muted text-foreground font-medium'
                  : isDanger
                    ? 'text-destructive/80 hover:bg-destructive/5 hover:text-destructive'
                    : 'text-muted-foreground hover:bg-muted/60 hover:text-foreground'}"
            >
              <SvelteIcon size={16} strokeWidth={1.75} aria-hidden="true" />
              <span>{$t(entry.labelKey)}</span>
            </button>
          </li>
        {/each}
      </ul>
    </div>
  {/each}
</nav>
