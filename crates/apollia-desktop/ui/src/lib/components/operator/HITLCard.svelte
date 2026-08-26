<script lang="ts">
  import { Shield, Check, Zap, ChevronDown } from "lucide-svelte";
  import { t } from "svelte-i18n";
  import { slide } from "svelte/transition";
  import StatusDot from "./StatusDot.svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";

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
    { labelKey: string; tone: "success" | "warning" | "danger"; color: string; bg: string }
  > = {
    low: {
      labelKey: "approval.risk.low",
      tone: "success",
      color: "hsl(var(--success))",
      bg: "hsl(var(--success) / 0.10)",
    },
    medium: {
      labelKey: "approval.risk.medium",
      tone: "warning",
      color: "hsl(var(--warning))",
      bg: "hsl(var(--warning) / 0.10)",
    },
    high: {
      labelKey: "approval.risk.high",
      tone: "danger",
      color: "hsl(var(--destructive))",
      bg: "hsl(var(--destructive) / 0.10)",
    },
    paid: {
      labelKey: "approval.risk.paid",
      tone: "danger",
      color: "hsl(var(--destructive))",
      bg: "hsl(var(--destructive) / 0.10)",
    },
  };

  const r = $derived(RISK_CONFIG[risk]);
</script>

<div
  class="rounded-lg overflow-hidden bg-card border border-border shadow-elev-1"
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
      <div class="text-code-sm font-semibold text-foreground">{action}</div>
      <div
        class="text-micro-lg text-muted-foreground mt-0.5 flex items-center gap-1.5 flex-wrap"
      >
        {#if agent}
          <span>{agent}</span>
          <span>·</span>
        {/if}
        {#if tool}
          <code
            class="font-mono text-micro-lg px-1.5 py-px rounded bg-primary/10 text-primary"
          >
            {tool}
          </code>
        {/if}
        {#if scope}
          <span>·</span><span>{scope}</span>
        {/if}
      </div>
    </div>
    <Badge variant={r.tone} size="sm">
      {#snippet icon()}<StatusDot color={r.color} />{/snippet}
      {$t("approval.risk_label", { values: { level: $t(r.labelKey) } })}
    </Badge>
  </div>
  <div class="px-3.5 py-2.5">
    {#if summary}
      <p class="text-body-xs text-muted-foreground leading-[1.6] mb-2">
        {summary}
      </p>
    {/if}
    {#if params.length > 0}
      <ul
        class="m-0 pl-[17px] text-body-xs leading-[1.7] text-muted-foreground"
      >
        {#each params as p}
          <li>{p}</li>
        {/each}
      </ul>
    {/if}
    {#if cost}
      <div
        class="mt-2 px-2.5 py-1.5 rounded-md bg-warning/10 text-warning-a11y text-caption-lg font-medium inline-flex items-center gap-1.5"
      >
        <Zap size={11} /> {$t("approval.cost_estimate", { values: { cost } })}
      </div>
    {/if}
  </div>
  <div
    class="px-3.5 py-2.5 border-t border-border/60 flex flex-wrap items-center gap-2"
  >
    <Button variant="primary-solid" size="sm" onclick={onApprove}>
      {#snippet icon()}<Check size={11} />{/snippet}
      {#snippet kbd()}<span class="font-mono text-micro opacity-80">↵</span>{/snippet}
      {$t("approval.action.allow")}
    </Button>
    <Button variant="outline" size="sm" onclick={onReject}>{$t("approval.action.refuse")}</Button>
    {#if onAlwaysAccept}
      <button
        type="button"
        class="inline-flex items-center gap-1 rounded-md px-2.5 py-1 text-caption text-primary hover:bg-primary/10 transition-colors"
        onclick={() => (scopeOpen = !scopeOpen)}
        aria-expanded={scopeOpen}
        data-testid="hitl-always-toggle"
      >
        {$t("approval.action.always_allow")}
        <ChevronDown
          size={11}
          class="transition-transform {scopeOpen ? 'rotate-180' : ''}"
        />
      </button>
    {/if}
    {#if expires}
      <span
        class="ml-auto text-micro-lg text-muted-foreground/70 font-mono"
      >
        {$t("approval.expires_in", { values: { expires } })}
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
        class="rounded-md px-2.5 py-1.5 text-left text-caption hover:bg-card transition-colors"
        onclick={() => { scopeOpen = false; onAlwaysAccept!("this_session"); }}
        data-testid="hitl-scope-session"
      >
        <div class="font-medium text-foreground">{$t("approval.scope.session_title")}</div>
        <div class="text-micro text-muted-foreground">
          {$t("approval.scope.session_desc")}
        </div>
      </button>
      <button
        type="button"
        class="rounded-md px-2.5 py-1.5 text-left text-caption hover:bg-card transition-colors"
        onclick={() => { scopeOpen = false; onAlwaysAccept!("this_agent"); }}
        data-testid="hitl-scope-agent"
      >
        <div class="font-medium text-foreground">{$t("approval.scope.agent_title")}</div>
        <div class="text-micro text-muted-foreground">
          {$t("approval.scope.agent_desc")}
        </div>
      </button>
      <button
        type="button"
        class="rounded-md px-2.5 py-1.5 text-left text-caption hover:bg-card transition-colors disabled:cursor-not-allowed disabled:opacity-50 disabled:hover:bg-transparent"
        disabled={!hasProject}
        onclick={() => { scopeOpen = false; onAlwaysAccept!("this_project"); }}
        data-testid="hitl-scope-project"
        title={!hasProject ? $t("approval.scope.project_no_project") : undefined}
      >
        <div class="font-medium text-foreground">{$t("approval.scope.project_title")}</div>
        <div class="text-micro text-muted-foreground">
          {hasProject
            ? $t("approval.scope.project_desc")
            : $t("approval.scope.project_unavailable")}
        </div>
      </button>
      <button
        type="button"
        class="rounded-md px-2.5 py-1.5 text-left text-caption hover:bg-card transition-colors"
        onclick={() => { scopeOpen = false; onAlwaysAccept!("global"); }}
        data-testid="hitl-scope-global"
      >
        <div class="font-medium text-foreground">{$t("approval.scope.global_title")}</div>
        <div class="text-micro text-warning-a11y">
          {$t("approval.scope.global_desc")}
        </div>
      </button>
    </div>
  {/if}
</div>
