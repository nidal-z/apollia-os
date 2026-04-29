<script lang="ts">
  /**
   * Next Steps panel — renders up to 3 Meta-LLM-generated
   * cards for either the operator Dashboard (`context="global_context"`)
   * or the end-of-session debrief (`context="session_end"`).
   *
   * The store handles caching, dismiss persistence, and feedback. This
   * component is the thin presentational layer + a whitelist-enforcing
   * action resolver so the LLM cannot steer the app anywhere it should
   * not go.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { invoke } from "@tauri-apps/api/core";
  import { Sparkles, RotateCw } from "lucide-svelte";
  import NextStepCard from "./NextStepCard.svelte";
  import { navigateTo, type Route } from "$lib/stores/navigation";
  import {
    nextSteps,
    type NextStep,
    type NextStepsContext,
    type NextStepsFacts,
    type NextStepsMode,
  } from "$lib/stores/nextSteps";

  interface Props {
    /** Unique cache key for this panel — e.g. `"global"` or `"session:<id>"`. */
    scopeKey: string;
    /** Which context the backend should narrate. */
    context: NextStepsContext;
    /** Persona — operator (default) or builder. */
    mode?: NextStepsMode;
    /** Frontend-computed facts passed verbatim to the backend. */
    facts: NextStepsFacts;
    /** Optional title override — defaults to `next_steps.title`. */
    title?: string;
  }

  let {
    scopeKey,
    context,
    mode = "operator",
    facts,
    title,
  }: Props = $props();

  const visible = $derived(nextSteps.visible(scopeKey));

  // ── Whitelists (mirror backend, last line of defence) ────────────────

  const ROUTE_WHITELIST: Record<string, { route: Route }> = {
    "/dashboard": { route: "dashboard" },
    "/agents": { route: "agents" },
    "/projects": { route: "projects" },
    "/tasks": { route: "tasks" },
    "/chat": { route: "chat" },
    "/automations": { route: "automations" },
    "/automations?wizard=open": { route: "automations" },
    "/integrations": { route: "integrations" },
    "/inbox": { route: "inbox" },
    "/llm": { route: "llm" },
    "/triggers": { route: "automations" },
    "/memory": { route: "memory" },
    "/memory?new": { route: "memory" },
    "/observability": { route: "observability" },
    "/notifications": { route: "notifications" },
    "/settings": { route: "settings" },
    "/templates": { route: "templates" },
    "/templates?filter=agents": { route: "templates" },
  };

  const COMMAND_WHITELIST = new Set([
    "memory_insert",
    "create_trigger",
    "install_agent",
    "export_session",
  ]);

  // ── Lifecycle ─────────────────────────────────────────────────────────

  onMount(() => {
    nextSteps.load(scopeKey, context, mode, facts);
  });

  // Re-fetch when facts change (cache-honoring — store guards the 12h TTL).
  let lastFactsKey = "";
  $effect(() => {
    const currentFacts = facts;
    const key = JSON.stringify(currentFacts);
    if (key !== lastFactsKey) {
      lastFactsKey = key;
      nextSteps.load(scopeKey, context, mode, currentFacts);
    }
  });

  function handleRefresh() {
    nextSteps.refresh(scopeKey, context, mode, facts);
  }

  // ── Action dispatch ──────────────────────────────────────────────────

  async function handleAction(step: NextStep) {
    const btn = step.actionButton;
    if (btn.action === "navigate") {
      const route = btn.payload?.route as string | undefined;
      const hit = route ? ROUTE_WHITELIST[route] : undefined;
      if (hit) {
        navigateTo(hit.route);
      } else {
        console.warn("next-steps: dropped non-whitelisted route", route);
      }
      return;
    }
    if (btn.action === "invoke") {
      const command = btn.payload?.command as string | undefined;
      if (!command || !COMMAND_WHITELIST.has(command)) {
        console.warn("next-steps: dropped non-whitelisted command", command);
        return;
      }
      const args = (btn.payload?.args as Record<string, unknown> | undefined) ?? {};
      try {
        await invoke(command, args);
      } catch (err) {
        console.warn("next-steps: invoke failed", command, err);
      }
    }
  }

  function handleDismiss(id: string) {
    nextSteps.dismiss(id);
  }

  function handleFeedback(id: string, value: "useful" | "not_useful") {
    nextSteps.setFeedback(id, value);
  }

  function feedbackFor(id: string) {
    return nextSteps.feedbackFor(id);
  }
</script>

<section
    class="glass-card glass-border rounded-lg p-4"
    data-testid="next-steps-panel"
    data-scope={scopeKey}
    aria-live="polite"
  >
    <header class="mb-3 flex items-center justify-between">
      <h2 class="flex items-center gap-1.5 text-sm font-medium uppercase tracking-wider text-muted-foreground">
        <Sparkles size={13} class="text-primary/70" />
        {title ?? $t("next_steps.title")}
      </h2>
      <button
        type="button"
        class="flex items-center gap-1 rounded-md px-2 py-1 text-[11px] text-muted-foreground/70 transition-colors hover:text-foreground disabled:opacity-40"
        aria-label={$t("next_steps.refresh")}
        data-testid="next-steps-refresh"
        disabled={$visible.loading}
        onclick={handleRefresh}
      >
        <RotateCw size={11} class={$visible.loading ? "animate-spin" : ""} />
        <span class="hidden sm:inline">{$t("next_steps.refresh")}</span>
      </button>
    </header>

    {#if $visible.loading && $visible.steps.length === 0}
      <div class="space-y-2" aria-busy="true">
        <div class="h-16 rounded-md bg-muted/30"></div>
        <div class="h-16 rounded-md bg-muted/20"></div>
      </div>
    {:else if $visible.error}
      <p class="text-xs text-muted-foreground" data-testid="next-steps-error">
        {$t("next_steps.error")}
      </p>
    {:else if $visible.steps.length === 0}
      <p class="text-xs text-muted-foreground" data-testid="next-steps-empty">
        {$t("next_steps.empty")}
      </p>
    {:else}
      <div class="flex flex-col gap-2 md:grid md:grid-cols-1" data-testid="next-steps-cards">
        {#each $visible.steps as step (step.id)}
          <NextStepCard
            {step}
            feedback={feedbackFor(step.id)}
            onaction={handleAction}
            ondismiss={handleDismiss}
            onfeedback={handleFeedback}
          />
        {/each}
      </div>
      {#if !$visible.fromLlm}
        <p class="mt-2 text-[10px] italic text-muted-foreground/60" data-testid="next-steps-fallback-flag">
          {$t("next_steps.fallback_hint")}
        </p>
      {/if}
    {/if}
</section>
