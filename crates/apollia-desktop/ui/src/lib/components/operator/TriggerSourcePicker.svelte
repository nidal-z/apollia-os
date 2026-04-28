<script lang="ts" module>
  import {
    Calendar,
    Repeat,
    Clock,
    FolderOpen,
    Globe,
  } from "lucide-svelte";

  export type TriggerSourceKind = "cron" | "interval" | "once" | "file" | "webhook";

  export type TriggerSourceDef = {
    k: TriggerSourceKind;
    label: string;
    desc: string;
    icon: typeof Calendar;
  };

  export const TRIGGER_SOURCES: TriggerSourceDef[] = [
    {
      k: "cron",
      label: "Sur un calendrier",
      desc: "Quotidien, hebdomadaire, à une heure précise",
      icon: Calendar,
    },
    {
      k: "interval",
      label: "À intervalle régulier",
      desc: "Toutes les 30 min, toutes les heures…",
      icon: Repeat,
    },
    {
      k: "once",
      label: "Une seule fois",
      desc: "Se déclenche à la date et l'heure indiquées",
      icon: Clock,
    },
    {
      k: "file",
      label: "Quand un fichier change",
      desc: "Réagit aux créations, modifications ou suppressions dans un dossier",
      icon: FolderOpen,
    },
    {
      k: "webhook",
      label: "Via une URL externe",
      desc: "Déclenché par n'importe quel service via HTTP POST",
      icon: Globe,
    },
  ];

  export type CronPreset = {
    k: string;
    label: string;
    expr: string;
  };

  export const CRON_PRESETS: CronPreset[] = [
    { k: "15min", label: "Toutes les 15 min", expr: "*/15 * * * *" },
    { k: "30min", label: "Toutes les 30 min", expr: "*/30 * * * *" },
    { k: "hour", label: "Toutes les heures", expr: "0 * * * *" },
    { k: "day", label: "Chaque jour à…", expr: "0 9 * * *" },
    { k: "week", label: "Chaque semaine…", expr: "0 9 * * 1" },
    { k: "custom", label: "Planification personnalisée", expr: "" },
  ];
</script>

<script lang="ts">
  import { Check } from "lucide-svelte";

  interface Props {
    value?: TriggerSourceKind;
    /** When source = cron, the chosen preset key. */
    cronPreset?: string;
    /** Show cron preset row. */
    showPresets?: boolean;
    onChange?: (value: TriggerSourceKind) => void;
    onCronPresetChange?: (preset: CronPreset) => void;
  }

  let {
    value = $bindable("cron"),
    cronPreset = $bindable("day"),
    showPresets = true,
    onChange,
    onCronPresetChange,
  }: Props = $props();
</script>

<div class="grid grid-cols-5 gap-3">
  {#each TRIGGER_SOURCES as s}
    {@const active = value === s.k}
    {@const Ic = s.icon}
    <button
      type="button"
      onclick={() => {
        value = s.k;
        onChange?.(s.k);
      }}
      class="px-3.5 py-4 rounded-xl cursor-pointer relative text-left transition-all {active
        ? 'bg-primary/10 border-2 border-primary shadow-elev-2'
        : 'bg-card border border-border hover:border-primary/40'}"
    >
      <div
        class="w-9 h-9 rounded-[9px] mb-2.5 inline-flex items-center justify-center {active
          ? 'bg-primary text-white'
          : 'bg-primary/10 text-primary'}"
      >
        <Ic size={17} />
      </div>
      <div class="text-[12.5px] font-semibold text-foreground mb-1">{s.label}</div>
      <div class="text-[10.5px] text-muted-foreground leading-[1.45]">{s.desc}</div>
      {#if active}
        <div
          class="absolute top-2.5 right-2.5 w-[18px] h-[18px] rounded-full bg-primary text-white inline-flex items-center justify-center"
        >
          <Check size={11} />
        </div>
      {/if}
    </button>
  {/each}
</div>

{#if showPresets && value === "cron"}
  <div class="mt-5">
    <div
      class="text-[10.5px] tracking-[1.2px] text-muted-foreground font-mono font-semibold uppercase mb-2"
    >
      Préréglages cron · sélection guidée
    </div>
    <div class="grid grid-cols-3 gap-2">
      {#each CRON_PRESETS as p}
        {@const active = cronPreset === p.k}
        <button
          type="button"
          onclick={() => {
            cronPreset = p.k;
            onCronPresetChange?.(p);
          }}
          class="px-3.5 py-2.5 rounded-[9px] cursor-pointer flex items-center gap-2.5 text-left transition-all {active
            ? 'bg-primary/10 border border-primary'
            : 'bg-card border border-border hover:border-primary/40'}"
        >
          <div
            class="w-3.5 h-3.5 rounded-full shrink-0 bg-white inline-flex items-center justify-center"
            style="border: 2px solid {active
              ? 'hsl(var(--primary))'
              : 'hsl(var(--muted-foreground)/0.5)'};"
          >
            {#if active}
              <div
                class="w-1.5 h-1.5 rounded-full bg-primary"
              ></div>
            {/if}
          </div>
          <div class="flex-1">
            <div class="text-[12px] font-medium text-foreground">{p.label}</div>
            {#if p.expr}
              <div class="text-[10.5px] text-muted-foreground font-mono mt-px">
                {p.expr}
              </div>
            {/if}
          </div>
        </button>
      {/each}
    </div>
  </div>
{/if}
