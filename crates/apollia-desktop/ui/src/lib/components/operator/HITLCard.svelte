<script lang="ts">
  import { Shield, Check, Zap, ChevronDown } from "lucide-svelte";
  import { slide } from "svelte/transition";
  import StatusDot from "./StatusDot.svelte";
  import Chip from "./Chip.svelte";
  import BtnPrimary from "./BtnPrimary.svelte";
  import BtnSecondary from "./BtnSecondary.svelte";

  export type RiskLevel = "low" | "medium" | "high" | "paid";
  export type AlwaysScope =
    | "this_session"
    | "this_agent"
    | "this_project"
    | "global";

  interface Props {
    /** Agent name initiating the action. */
    agent?: string;
    /** Headline action description. */
    action: string;
    risk: RiskLevel;
    /** Tool name (mono-display). */
    tool?: string;
    /** Scope description shown next to tool name. */
    scope?: string;
    /** Free-form summary text. */
    summary?: string;
    /** List of param/detail bullets. */
    params?: string[];
    /** Cost estimate string when risk === "paid". */
    cost?: string;
    /** Time-until-expiry label (e.g. "4m"). */
    expires?: string;
    onApprove?: (e: MouseEvent) => void;
    onReject?: (e: MouseEvent) => void;
    /** When set, surfaces the "Toujours autoriser" scope picker (chat tool flows). */
    onAlwaysAccept?: (scope: AlwaysScope) => void;
    /** Disables the "Toujours pour ce projet" option when the session has no project. */
    hasProject?: boolean;
  }

  let {
    agent,
    action,
    risk,
    tool,
    scope,
    summary,
    params = [],
    cost,
    expires,
    onApprove,
    onReject,
    onAlwaysAccept,
    hasProject = false,
  }: Props = $props();

  let scopeOpen = $state(false);

  const RISK_CONFIG: Record<
    RiskLevel,
    { label: string; tone: "success" | "warning" | "danger"; color: string; bg: string }
  > = {
    low: {
      label: "faible",
      tone: "success",
      color: "hsl(var(--success))",
      bg: "hsl(var(--success) / 0.10)",
    },
    medium: {
      label: "moyen",
      tone: "warning",
      color: "hsl(var(--warning))",
      bg: "hsl(var(--warning) / 0.10)",
    },
    high: {
      label: "élevé",
      tone: "danger",
      color: "hsl(var(--destructive))",
      bg: "hsl(var(--destructive) / 0.10)",
    },
    paid: {
      label: "payant",
      tone: "danger",
      color: "hsl(var(--destructive))",
      bg: "hsl(var(--destructive) / 0.10)",
    },
  };

  const r = $derived(RISK_CONFIG[risk]);
</script>

<div
  class="rounded-[10px] overflow-hidden bg-card border border-border shadow-elev-1"
>
  <div
    class="px-3.5 py-2.5 border-b border-border/60 flex items-center gap-2.5"
  >
    <div
      class="w-6 h-6 rounded-md inline-flex items-center justify-center shrink-0"
      style="background: {r.bg}; color: {r.color};"
    >
      <Shield size={12} />
    </div>
    <div class="flex-1 min-w-0">
      <div class="text-[12.5px] font-semibold text-foreground">{action}</div>
      <div
        class="text-[10.5px] text-muted-foreground mt-0.5 flex items-center gap-1.5 flex-wrap"
      >
        {#if agent}
          <span>{agent}</span>
          <span>·</span>
        {/if}
        {#if tool}
          <code
            class="font-mono text-[10.5px] px-1.5 py-px rounded bg-primary/10 text-primary"
          >
            {tool}
          </code>
        {/if}
        {#if scope}
          <span>·</span><span>{scope}</span>
        {/if}
      </div>
    </div>
    <Chip tone={r.tone} size="sm">
      {#snippet icon()}<StatusDot color={r.color} />{/snippet}
      risque {r.label}
    </Chip>
  </div>
  <div class="px-3.5 py-2.5">
    {#if summary}
      <p class="text-[12px] text-muted-foreground leading-[1.6] mb-2">
        {summary}
      </p>
    {/if}
    {#if params.length > 0}
      <ul
        class="m-0 pl-[17px] text-[12px] leading-[1.7] text-muted-foreground"
      >
        {#each params as p}
          <li>{p}</li>
        {/each}
      </ul>
    {/if}
    {#if cost}
      <div
        class="mt-2 px-2.5 py-1.5 rounded-md bg-warning/10 text-warning-a11y text-[11.5px] font-medium inline-flex items-center gap-1.5"
      >
        <Zap size={11} /> Coût estimé : {cost}
      </div>
    {/if}
  </div>
  <div
    class="px-3.5 py-2.5 border-t border-border/60 flex flex-wrap items-center gap-2"
  >
    <BtnPrimary onclick={onApprove}>
      {#snippet icon()}<Check size={11} />{/snippet}
      {#snippet kbd()}<span class="font-mono text-[10px] opacity-80">↵</span>{/snippet}
      Autoriser
    </BtnPrimary>
    <BtnSecondary onclick={onReject}>Refuser</BtnSecondary>
    {#if onAlwaysAccept}
      <button
        type="button"
        class="inline-flex items-center gap-1 rounded-md px-2.5 py-1 text-[11px] text-primary hover:bg-primary/10 transition-colors"
        onclick={() => (scopeOpen = !scopeOpen)}
        aria-expanded={scopeOpen}
        data-testid="hitl-always-toggle"
      >
        Toujours autoriser
        <ChevronDown
          size={11}
          class="transition-transform {scopeOpen ? 'rotate-180' : ''}"
        />
      </button>
    {/if}
    {#if expires}
      <span
        class="ml-auto text-[10.5px] text-muted-foreground/70 font-mono"
      >
        expire dans {expires}
      </span>
    {/if}
  </div>
  {#if onAlwaysAccept && scopeOpen}
    <div
      class="px-3.5 py-2 border-t border-border/60 grid grid-cols-1 gap-1.5 sm:grid-cols-2 bg-muted/20"
      transition:slide={{ duration: 150 }}
      data-testid="hitl-scope-menu"
    >
      <button
        type="button"
        class="rounded-md px-2.5 py-1.5 text-left text-[11px] hover:bg-card transition-colors"
        onclick={() => { scopeOpen = false; onAlwaysAccept!("this_session"); }}
        data-testid="hitl-scope-session"
      >
        <div class="font-medium text-foreground">Pour cette session</div>
        <div class="text-[10px] text-muted-foreground">
          Jusqu'à la fermeture de ce chat.
        </div>
      </button>
      <button
        type="button"
        class="rounded-md px-2.5 py-1.5 text-left text-[11px] hover:bg-card transition-colors"
        onclick={() => { scopeOpen = false; onAlwaysAccept!("this_agent"); }}
        data-testid="hitl-scope-agent"
      >
        <div class="font-medium text-foreground">Toujours pour cet assistant</div>
        <div class="text-[10px] text-muted-foreground">
          L'assistant courant ne demandera plus.
        </div>
      </button>
      <button
        type="button"
        class="rounded-md px-2.5 py-1.5 text-left text-[11px] hover:bg-card transition-colors disabled:cursor-not-allowed disabled:opacity-50 disabled:hover:bg-transparent"
        disabled={!hasProject}
        onclick={() => { scopeOpen = false; onAlwaysAccept!("this_project"); }}
        data-testid="hitl-scope-project"
        title={!hasProject ? "Cette session n'est rattachée à aucun projet." : undefined}
      >
        <div class="font-medium text-foreground">Toujours pour ce projet</div>
        <div class="text-[10px] text-muted-foreground">
          {hasProject
            ? "Tous les assistants utilisés dans ce projet."
            : "Indisponible — la session n'est rattachée à aucun projet."}
        </div>
      </button>
      <button
        type="button"
        class="rounded-md px-2.5 py-1.5 text-left text-[11px] hover:bg-card transition-colors"
        onclick={() => { scopeOpen = false; onAlwaysAccept!("global"); }}
        data-testid="hitl-scope-global"
      >
        <div class="font-medium text-foreground">Toujours, partout</div>
        <div class="text-[10px] text-warning-a11y">
          Tous les assistants, tous les projets.
        </div>
      </button>
    </div>
  {/if}
</div>
