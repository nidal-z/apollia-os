<script lang="ts">
  import { Folder, Check, Shield, Sparkles } from "lucide-svelte";
  import StatusDot from "./StatusDot.svelte";

  export type ProjectStatus = "active" | "pause" | "blocked" | "done";

  interface Props {
    title: string;
    description?: string;
    status: ProjectStatus;
    /** Number of agents engaged. */
    agentCount?: number;
    /** Optional list of agent names for the avatar stack. */
    agents?: string[];
    /** Display label for last activity (e.g. "il y a 2 min"). */
    lastActivity?: string;
    /** Progress 0-100. */
    progress?: number;
    /** Next action (CTA) shown when blocked. */
    nextAction?: string;
    /** When status === active and the project is currently working. */
    live?: boolean;
    /** Hover affordance (used in zooms). */
    hover?: boolean;
    /** Quick metrics: e.g. ["8 conversations", "12 tâches"]. */
    metrics?: string[];
    /** Accent color (CSS string). Defaults to primary. */
    color?: string;
    onclick?: (e: MouseEvent) => void;
  }

  let {
    title,
    description,
    status,
    agentCount,
    agents = [],
    lastActivity,
    progress,
    nextAction,
    live = false,
    hover = false,
    metrics = [],
    color = "hsl(var(--primary))",
    onclick,
  }: Props = $props();

  const isPause = $derived(status === "pause");
  const isBlocked = $derived(status === "blocked");
  const isDone = $derived(status === "done");

  const accentVar = $derived(
    isPause
      ? "hsl(var(--muted-foreground))"
      : isBlocked
        ? "hsl(var(--destructive))"
        : isDone
          ? "hsl(var(--success))"
          : color,
  );

  const statusLabel = $derived(
    status === "active"
      ? "actif"
      : status === "pause"
        ? "en pause"
        : status === "blocked"
          ? "bloqué"
          : "terminé",
  );
</script>

<div
  role={onclick ? "button" : undefined}
  tabindex={onclick ? 0 : undefined}
  onclick={onclick}
  onkeydown={onclick
    ? (e) => {
        if (e.key === "Enter" || e.key === " ") onclick(e as unknown as MouseEvent);
      }
    : undefined}
  class="bg-card border border-border rounded-xl overflow-hidden cursor-pointer transition-all {hover
    ? 'hover-lift'
    : ''}"
>
  <div class="h-[5px]" style="background: {accentVar};"></div>
  <div class="px-[15px] py-[13px]">
    <div class="flex items-start gap-2.5 mb-2.5">
      <div
        class="w-7 h-7 rounded-md inline-flex items-center justify-center shrink-0"
        style="background: {isPause
          ? 'hsl(var(--muted))'
          : isBlocked
            ? 'hsl(var(--destructive) / 0.10)'
            : isDone
              ? 'hsl(var(--success) / 0.10)'
              : `${color}20`}; color: {accentVar};"
      >
        {#if isDone}
          <Check size={13} />
        {:else if isBlocked}
          <Shield size={13} />
        {:else}
          <Folder size={13} />
        {/if}
      </div>
      <div class="flex-1 min-w-0">
        <div
          class="text-[13px] font-semibold text-foreground"
          style:text-decoration={isDone ? "line-through" : "none"}
          style:text-decoration-color="hsl(var(--muted-foreground)/0.6)"
        >
          {title}
        </div>
        <div
          class="text-[10.5px] text-muted-foreground mt-0.5 inline-flex items-center gap-1.5"
        >
          {#if live && !isDone && !isBlocked && !isPause}
            <StatusDot color="hsl(var(--primary))" glow />
          {/if}
          <span>{statusLabel}{lastActivity ? ` · ${lastActivity}` : ""}</span>
        </div>
      </div>
    </div>

    {#if description}
      <p class="text-[11.5px] text-muted-foreground mb-2 leading-[1.45]">
        {description}
      </p>
    {/if}

    {#if isBlocked && nextAction}
      <div
        class="text-[10.5px] text-danger-a11y bg-destructive/10 px-2 py-1.5 rounded-md mb-2.5 inline-flex items-center gap-1.5"
      >
        <Shield size={10} />
        {nextAction}
      </div>
    {/if}

    {#if metrics.length > 0}
      <div
        class="flex gap-2.5 text-[10.5px] text-muted-foreground mb-2.5"
      >
        {#each metrics as m}
          <span>{m}</span>
        {/each}
      </div>
    {/if}

    {#if progress !== undefined}
      <div class="mb-2.5">
        <div
          class="h-1 rounded-full bg-muted overflow-hidden"
        >
          <div
            class="h-full transition-all"
            style="width: {Math.max(0, Math.min(100, progress))}%; background: {accentVar};"
          ></div>
        </div>
        <div
          class="text-[10px] text-muted-foreground/80 mt-1 font-mono"
        >
          {Math.round(progress)}%{isDone ? " · livré" : ""}
        </div>
      </div>
    {/if}

    {#if agents.length > 0 || agentCount}
      <div
        class="flex items-center gap-1 pt-2 border-t border-border/50"
      >
        <div class="flex">
          {#each agents.slice(0, 3) as _, i}
            <div
              class="w-[18px] h-[18px] rounded-full inline-flex items-center justify-center border-2 border-card"
              style="background: linear-gradient(135deg, hsl(var(--primary)), hsl(var(--secondary))); margin-left: {i ===
              0
                ? 0
                : -6}px;"
            >
              <Sparkles size={8} color="white" />
            </div>
          {/each}
        </div>
        <span
          class="text-[10px] text-muted-foreground ml-1 truncate"
        >
          {agents.length > 0
            ? agents.join(", ")
            : `${agentCount} agent${(agentCount ?? 0) > 1 ? "s" : ""}`}
        </span>
      </div>
    {/if}
  </div>
</div>
