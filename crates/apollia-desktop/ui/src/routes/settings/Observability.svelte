<script lang="ts" context="module">
  /**
   * Settings → Observability - fenêtre de la config `[observability]` du
   * fichier `apollia.toml`.
   *
   * Pour la phase MVP, on affiche les valeurs courantes en lecture seule
   * et on offre un bouton « Modifier dans apollia.toml » qui ouvre le
   * fichier dans l'éditeur système. Le CRUD live des toggles arrivera
   * dans un Lot ultérieur - préserver le fichier comme source de vérité
   * a moins de risque qu'un système de double écriture.
   */
  export const meta = {
    title: "settings.nav.observability",
    icon: "activity",
    group: "settings.nav.cluster_system",
    cluster: "system",
  } as const;
</script>

<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { Activity, ExternalLink } from "lucide-svelte";

  /**
   * Vue plate de la config - lit les valeurs depuis `apollia.toml` via la
   * commande Tauri `get_config` quand celle-ci sera étendue. En attendant,
   * on affiche les defaults documentés.
   */
  interface ObservabilityConfigView {
    captureThoughts: boolean;
    captureLlmPrompts: boolean;
    captureToolArgs: boolean;
    captureToolOutputs: boolean;
    captureAgentLogs: boolean;
    retentionDays: number;
  }

  // Defaults alignés sur ObservabilityConfig::default (apollia-core).
  const DEFAULTS: ObservabilityConfigView = {
    captureThoughts: true,
    captureLlmPrompts: true,
    captureToolArgs: true,
    captureToolOutputs: true,
    captureAgentLogs: true,
    retentionDays: 90,
  };

  let config = $state<ObservabilityConfigView>(DEFAULTS);

  async function openConfig() {
    try {
      await invoke("open_config_in_editor");
    } catch (e) {
      console.error("failed to open apollia.toml", e);
    }
  }

  onMount(() => {
    // FUTURE: extend the Rust `get_config` command to expose the
    // `[observability]` section so we can hydrate `config` from disk.
    // For now we stick to the defaults so we do not display misleading
    // values to the user.
  });

  const SETTINGS = [
    {
      key: "captureThoughts" as const,
      labelKey: "settings.observability.capture_thoughts",
      descKey: "settings.observability.capture_thoughts_desc",
    },
    {
      key: "captureLlmPrompts" as const,
      labelKey: "settings.observability.capture_llm_prompts",
      descKey: "settings.observability.capture_llm_prompts_desc",
    },
    {
      key: "captureToolArgs" as const,
      labelKey: "settings.observability.capture_tool_args",
      descKey: "settings.observability.capture_tool_args_desc",
    },
    {
      key: "captureToolOutputs" as const,
      labelKey: "settings.observability.capture_tool_outputs",
      descKey: "settings.observability.capture_tool_outputs_desc",
    },
    {
      key: "captureAgentLogs" as const,
      labelKey: "settings.observability.capture_agent_logs",
      descKey: "settings.observability.capture_agent_logs_desc",
    },
  ];
</script>

<section class="space-y-5" data-testid="observability-settings">
  <header class="flex items-start gap-3">
    <div class="rounded-lg bg-primary/10 p-2 text-primary">
      <Activity size={16} />
    </div>
    <div>
      <h2 class="text-base font-semibold text-foreground">
        {$t("settings.observability.title")}
      </h2>
      <p class="mt-0.5 text-sm text-muted-foreground">
        {$t("settings.observability.subtitle")}
      </p>
    </div>
  </header>

  <div class="rounded-lg border border-border/40 bg-surface-1/30 p-4 space-y-3">
    {#each SETTINGS as setting (setting.key)}
      <div class="flex items-start justify-between gap-4">
        <div class="min-w-0 flex-1">
          <div class="text-sm font-medium text-foreground">
            {$t(setting.labelKey)}
          </div>
          <div class="mt-0.5 text-xs text-muted-foreground">
            {$t(setting.descKey)}
          </div>
        </div>
        <div
          class="shrink-0 inline-flex items-center gap-1.5 px-2 py-0.5 rounded text-[11px] font-mono {config[
            setting.key
          ]
            ? 'bg-success/10 text-success'
            : 'bg-muted text-muted-foreground'}"
        >
          {config[setting.key] ? "ON" : "OFF"}
        </div>
      </div>
    {/each}
    <div class="flex items-center justify-between gap-4 pt-3 border-t border-border/40">
      <div class="min-w-0 flex-1">
        <div class="text-sm font-medium text-foreground">
          {$t("settings.observability.retention_days")}
        </div>
        <div class="mt-0.5 text-xs text-muted-foreground">
          {$t("settings.observability.retention_days_desc")}
        </div>
      </div>
      <div class="shrink-0 inline-flex items-center gap-1.5 px-2 py-0.5 rounded text-[11px] font-mono bg-muted text-muted-foreground">
        {config.retentionDays} d
      </div>
    </div>
  </div>

  <div class="rounded-lg border border-warning/20 bg-warning/5 p-3 text-xs text-muted-foreground">
    {$t("settings.observability.edit_via_toml_hint")}
    <button
      type="button"
      onclick={openConfig}
      class="mt-2 inline-flex items-center gap-1.5 rounded-md border border-border/60 px-2.5 py-1.5 text-xs font-medium text-foreground hover:bg-surface-2 transition-colors"
    >
      <ExternalLink size={12} />
      {$t("settings.observability.open_toml")}
    </button>
  </div>
</section>
