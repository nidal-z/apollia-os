<script lang="ts">
  import {
    Calendar,
    Repeat,
    Clock,
    FolderOpen,
    Globe,
    AlertTriangle,
    X,
    Play,
    History,
    Edit,
    Trash2,
  } from "lucide-svelte";
  import StatusDot from "./StatusDot.svelte";
  import Chip from "./Chip.svelte";

  export type TriggerSource =
    | "cron"
    | "interval"
    | "once"
    | "file"
    | "webhook"
    | "error";

  export type TriggerStatus = "ok" | "paused" | "error" | "scheduled";

  export type Trigger = {
    id: string;
    name?: string;
    source: TriggerSource;
    /** Raw expression or path. */
    expression: string;
    /** Human-readable description. */
    human?: string;
    agent?: string;
    fires?: number;
    ignored?: number;
    status: TriggerStatus;
    errorMsg?: string;
    /** Toggle on/off (active = currently armed). */
    active?: boolean;
    lastFired?: string;
    nextFire?: string;
  };

  interface Props {
    trigger: Trigger;
    onToggle?: (active: boolean) => void;
    onFire?: () => void;
    onLogs?: () => void;
    onEdit?: () => void;
    onDelete?: () => void;
  }

  let { trigger, onToggle, onFire, onLogs, onEdit, onDelete }: Props = $props();

  const SRC_ICON = {
    cron: Calendar,
    interval: Repeat,
    once: Clock,
    file: FolderOpen,
    webhook: Globe,
    error: AlertTriangle,
  };

  const STATUS = {
    ok: { color: "hsl(var(--success))", tone: "success" as const, label: "actif", glow: true },
    paused: {
      color: "hsl(var(--muted-foreground))",
      tone: "neutral" as const,
      label: "en pause",
      glow: false,
    },
    error: {
      color: "hsl(var(--destructive))",
      tone: "danger" as const,
      label: "erreur",
      glow: true,
    },
    scheduled: {
      color: "hsl(var(--info))",
      tone: "info" as const,
      label: "planifié",
      glow: false,
    },
  };

  const SrcIcon = $derived(SRC_ICON[trigger.source]);
  const cfg = $derived(STATUS[trigger.status]);
</script>

<div
  class="bg-card border border-border rounded-xl overflow-hidden hover-lift transition-all"
>
  <div
    class="h-0.5 transition-opacity"
    style="background: {cfg.color}; opacity: {trigger.status === 'paused' ? 0.3 : 1};"
  ></div>
  <div class="px-4 py-3">
    <div class="flex items-center gap-2.5">
      <div
        class="w-8 h-8 rounded-[9px] shrink-0 inline-flex items-center justify-center bg-primary/10 text-primary"
      >
        <SrcIcon size={15} />
      </div>
      <div class="flex-1 min-w-0">
        <div class="flex items-center gap-2 flex-wrap">
          <span class="text-[13px] font-semibold text-foreground font-mono">
            {trigger.id}
          </span>
          <Chip size="sm" tone="neutral">{trigger.source.toUpperCase()}</Chip>
          <Chip size="sm" tone={cfg.tone}>
            {#snippet icon()}<StatusDot color={cfg.color} glow={cfg.glow} />{/snippet}
            {cfg.label}
          </Chip>
        </div>
        {#if trigger.human}
          <div class="text-[11.5px] text-muted-foreground mt-1">{trigger.human}</div>
        {/if}
      </div>
      {#if trigger.nextFire}
        <span
          class="text-[10.5px] text-muted-foreground font-mono"
        >
          prochain :
          <strong class="text-foreground font-semibold">{trigger.nextFire}</strong>
        </span>
      {/if}
      <button
        type="button"
        onclick={() => onToggle?.(!trigger.active)}
        aria-label="Activer/désactiver"
        class="w-9 h-5 rounded-full p-0.5 cursor-pointer flex items-center transition-colors {trigger.active
          ? 'bg-primary'
          : 'bg-border'}"
      >
        <div
          class="w-4 h-4 rounded-full bg-white shadow-elev-1 transition-transform"
          style:transform={trigger.active ? "translateX(16px)" : "translateX(0)"}
        ></div>
      </button>
    </div>

    <div class="flex items-center gap-4 mt-2.5 pl-[42px] flex-wrap">
      <code
        class="font-mono text-[11px] px-2 py-0.5 rounded-md bg-surface-2 text-foreground border border-border"
      >
        {trigger.expression}
      </code>
      {#if trigger.fires !== undefined}
        <span class="text-[11px] text-muted-foreground">
          déclench. : <strong class="text-foreground font-semibold">{trigger.fires}</strong>
        </span>
      {/if}
      {#if trigger.ignored !== undefined}
        <span class="text-[11px] text-muted-foreground">
          ignorés : <strong class="text-foreground font-semibold">{trigger.ignored}</strong>
        </span>
      {/if}
      {#if trigger.errorMsg}
        <span
          class="text-[11px] text-danger-a11y inline-flex items-center gap-1.5"
        >
          <X size={11} />
          {trigger.errorMsg}
        </span>
      {/if}
      <div class="flex-1"></div>
      <div class="flex gap-1">
        <button
          type="button"
          onclick={onFire}
          class="px-2.5 py-1 rounded-md border-0 bg-primary text-white text-[11px] font-medium cursor-pointer inline-flex items-center gap-1 hover:bg-primary/90"
        >
          <Play size={11} /> Déclencher
        </button>
        <button
          type="button"
          onclick={onLogs}
          class="px-2.5 py-1 rounded-md bg-transparent border border-border text-muted-foreground text-[11px] font-medium cursor-pointer inline-flex items-center gap-1 hover:bg-muted"
        >
          <History size={11} /> Logs
        </button>
        <button
          type="button"
          onclick={onEdit}
          class="px-2.5 py-1 rounded-md bg-transparent border border-border text-muted-foreground text-[11px] font-medium cursor-pointer inline-flex items-center gap-1 hover:bg-muted"
        >
          <Edit size={11} /> Modifier
        </button>
        <button
          type="button"
          onclick={onDelete}
          class="px-2.5 py-1 rounded-md bg-transparent border border-border text-danger-a11y text-[11px] font-medium cursor-pointer inline-flex items-center gap-1 hover:bg-destructive/10"
        >
          <Trash2 size={11} /> Supprimer
        </button>
      </div>
    </div>
  </div>
</div>
