<script lang="ts">
  /**
   * Renders permission rule proposals (one card per rule) emitted by the
   * onboarding agent under the memory key `onboarding.proposed_rules`.
   *
   * Each card lets the user apply or dismiss one proposal. Apply calls
   * `apply_proposed_permission_rule` (which persists directly to
   * `governance.db`), dismiss calls `dismiss_proposed_permission_rule`.
   * When the list becomes empty, fires `oncomplete`.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { ShieldCheck, ShieldX, Check, X } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { addToast } from "$lib/components/ui/toast/store";
  import { reportError } from "$lib/errors/reportError";
  import { Spinner } from "$lib/components/ui/progress";
  import {
    applyProposedPermissionRule,
    dismissProposedPermissionRule,
    listProposedPermissionRules,
    type ProposedRuleView,
  } from "$lib/ipc/onboarding";

  interface Props {
    /** Fires once the list of pending proposals reaches zero. */
    oncomplete: () => void;
  }

  const { oncomplete }: Props = $props();

  let pending = $state<ProposedRuleView[]>([]);
  let loading = $state(true);
  let busy = $state<Record<number, "applying" | "dismissing" | undefined>>({});

  async function refresh(): Promise<void> {
    try {
      pending = await listProposedPermissionRules();
    } catch (err) {
      reportError(err, { surface: "toast" });
      pending = [];
    } finally {
      loading = false;
      if (pending.length === 0) {
        oncomplete();
      }
    }
  }

  onMount(() => {
    void refresh();
  });

  async function apply(rule: ProposedRuleView): Promise<void> {
    busy = { ...busy, [rule.index]: "applying" };
    try {
      await applyProposedPermissionRule(rule.index);
      addToast($t("onboarding_permissions.rule_applied"), "success");
    } catch (err) {
      reportError(err, { surface: "toast" });
    } finally {
      busy = { ...busy, [rule.index]: undefined };
      await refresh();
    }
  }

  async function dismiss(rule: ProposedRuleView): Promise<void> {
    busy = { ...busy, [rule.index]: "dismissing" };
    try {
      await dismissProposedPermissionRule(rule.index);
    } catch (err) {
      reportError(err, { surface: "toast" });
    } finally {
      busy = { ...busy, [rule.index]: undefined };
      await refresh();
    }
  }

  function actionPill(action: string): { label: string; cls: string; allow: boolean } {
    if (action === "allow") {
      return {
        label: $t("onboarding_permissions.action_allow"),
        cls: "pill-allow",
        allow: true,
      };
    }
    return {
      label: $t("onboarding_permissions.action_deny"),
      cls: "pill-deny",
      allow: false,
    };
  }
</script>

<div class="permission-step" data-testid="onboarding-permission-step">
  <header class="step-header">
    <div class="step-icon">
      <ShieldCheck size={20} strokeWidth={1.75} aria-hidden="true" />
    </div>
    <div class="step-text">
      <p class="step-title">{$t("onboarding_permissions.title")}</p>
      <!-- subtitle includes a single <strong>; rendering as html is safe (no user input) -->
      <p class="step-subtitle">
        {@html $t("onboarding_permissions.subtitle_html")}
      </p>
    </div>
    {#if !loading && pending.length > 0}
      <span class="count-badge" aria-label={$t("onboarding_permissions.count_aria")}>
        {pending.length}
      </span>
    {/if}
  </header>

  {#if loading}
    <div class="empty">
      <Spinner size={16} /> {$t("onboarding_permissions.loading")}
    </div>
  {:else if pending.length === 0}
    <div class="empty">{$t("onboarding_permissions.empty")}</div>
  {:else}
    <ul class="rule-list">
      {#each pending as rule (rule.index)}
        {@const pill = actionPill(rule.action)}
        <li class="rule-card" class:rule-card-allow={pill.allow} class:rule-card-deny={!pill.allow}>
          <div class="rule-head">
            <span class={`pill ${pill.cls}`}>
              {#if pill.allow}
                <ShieldCheck size={12} aria-hidden="true" />
              {:else}
                <ShieldX size={12} aria-hidden="true" />
              {/if}
              {pill.label}
            </span>
            <span class="rule-tool" title={$t("onboarding_permissions.tool_title")}>{rule.tool_name}</span>
            <span class="rule-scope" title={$t("onboarding_permissions.scope_title")}>
              {$t("onboarding_permissions.scope_label", { values: { scope: rule.scope } })}
            </span>
          </div>

          {#if rule.arg_prefix}
            <div class="rule-target">
              <span class="rule-target-label">{$t("onboarding_permissions.prefix_title")}</span>
              <code class="rule-prefix">{rule.arg_prefix}</code>
            </div>
          {/if}

          <div class="rule-actions">
            <Button
              size="sm"
              variant="ghost"
              disabled={busy[rule.index] !== undefined}
              onclick={() => dismiss(rule)}
              data-testid={`onboarding-rule-dismiss-${rule.index}`}
            >
              {#if busy[rule.index] === "dismissing"}
                <Spinner size={14} />
              {:else}
                <X size={14} />
              {/if}
              {$t("onboarding_permissions.dismiss")}
            </Button>
            <Button
              size="sm"
              variant="primary-gradient"
              disabled={busy[rule.index] !== undefined}
              onclick={() => apply(rule)}
              data-testid={`onboarding-rule-approve-${rule.index}`}
            >
              {#if busy[rule.index] === "applying"}
                <Spinner size={14} />
              {:else}
                <Check size={14} />
              {/if}
              {$t("onboarding_permissions.apply")}
            </Button>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .permission-step {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    padding: 1.25rem 1.5rem 1.5rem;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .step-header {
    display: flex;
    gap: 0.75rem;
    align-items: flex-start;
  }

  .step-icon {
    width: 2.25rem;
    height: 2.25rem;
    border-radius: 999px;
    background: hsl(var(--primary) / 0.12);
    color: hsl(var(--primary));
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .step-text {
    flex: 1;
    min-width: 0;
  }

  .step-title {
    margin: 0;
    font-size: 0.9375rem;
    font-weight: 600;
    color: hsl(var(--foreground));
    letter-spacing: -0.01em;
  }

  .step-subtitle {
    margin: 0.1875rem 0 0 0;
    font-size: 0.8125rem;
    line-height: 1.45;
    color: hsl(var(--muted-foreground));
  }

  .count-badge {
    flex-shrink: 0;
    min-width: 1.5rem;
    height: 1.5rem;
    padding: 0 0.5rem;
    border-radius: 999px;
    background: hsl(var(--primary) / 0.14);
    color: hsl(var(--primary));
    font-size: 0.75rem;
    font-weight: 600;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .rule-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.625rem;
  }

  .rule-card {
    display: flex;
    flex-direction: column;
    gap: 0.625rem;
    padding: 0.875rem 1rem;
    border-radius: 0.625rem;
    border: 1px solid hsl(var(--border) / 0.7);
    background: hsl(var(--card));
    position: relative;
    overflow: hidden;
  }

  /* Left accent stripe colour-codes the proposed action at a glance. */
  .rule-card::before {
    content: "";
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    width: 3px;
  }
  .rule-card-allow::before {
    background: hsl(var(--success));
  }
  .rule-card-deny::before {
    background: hsl(var(--destructive));
  }

  .rule-head {
    display: flex;
    align-items: center;
    gap: 0.625rem;
    flex-wrap: wrap;
    min-width: 0;
  }

  .pill {
    display: inline-flex;
    align-items: center;
    gap: 0.3125rem;
    padding: 0.1875rem 0.5625rem;
    border-radius: 999px;
    font-size: 0.6875rem;
    font-weight: 600;
    border: 1px solid transparent;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    flex-shrink: 0;
  }

  .pill-allow {
    background: hsl(var(--success) / 0.12);
    color: hsl(var(--success-foreground));
    border-color: hsl(var(--success) / 0.3);
  }
  :global(.dark) .pill-allow {
    color: hsl(var(--success));
  }

  .pill-deny {
    background: hsl(var(--destructive) / 0.12);
    color: hsl(var(--destructive));
    border-color: hsl(var(--destructive) / 0.3);
  }

  .rule-tool {
    font-family:
      ui-monospace, SFMono-Regular, "Menlo", "Monaco", "Consolas", "Liberation Mono",
      "Courier New", monospace;
    font-size: 0.8125rem;
    font-weight: 600;
    color: hsl(var(--foreground));
    flex-shrink: 0;
  }

  .rule-scope {
    margin-left: auto;
    font-size: 0.6875rem;
    color: hsl(var(--muted-foreground));
    font-variant: small-caps;
    letter-spacing: 0.04em;
    flex-shrink: 0;
  }

  .rule-target {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-width: 0;
  }

  .rule-target-label {
    font-size: 0.6875rem;
    color: hsl(var(--muted-foreground));
    text-transform: uppercase;
    letter-spacing: 0.04em;
    flex-shrink: 0;
  }

  .rule-prefix {
    flex: 1;
    min-width: 0;
    padding: 0.25rem 0.5rem;
    border-radius: 0.3125rem;
    background: hsl(var(--muted) / 0.7);
    border: 1px solid hsl(var(--border) / 0.5);
    font-family:
      ui-monospace, SFMono-Regular, "Menlo", "Monaco", "Consolas", "Liberation Mono",
      "Courier New", monospace;
    font-size: 0.75rem;
    color: hsl(var(--foreground));
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .rule-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 0.5rem;
    padding-top: 0.125rem;
  }

  .empty {
    padding: 2rem 0.75rem;
    text-align: center;
    color: hsl(var(--muted-foreground));
    font-size: 0.8125rem;
    display: inline-flex;
    align-items: center;
    gap: 0.375rem;
    justify-content: center;
  }

  :global(.anim-spin) {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from {
      transform: rotate(0deg);
    }
    to {
      transform: rotate(360deg);
    }
  }
</style>
