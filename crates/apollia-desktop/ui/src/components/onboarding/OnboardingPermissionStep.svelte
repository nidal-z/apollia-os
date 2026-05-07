<script lang="ts">
  /**
   * Renders permission rule proposals (one card per rule) emitted by the
   * onboarding agent under the memory key `onboarding.proposed_rules`.
   *
   * Each card lets the user approve or refuse one proposal. Approval calls
   * `apply_proposed_permission_rule` (which persists directly to
   * `governance.db`), refusal calls `dismiss_proposed_permission_rule`.
   * When the list becomes empty, fires `oncomplete`.
   */
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { ShieldCheck, ShieldX, Check, X, Loader2 } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { addToast } from "$lib/components/ui/toast/store";

  interface ProposedRuleView {
    index: number;
    tool_name: string;
    action: string;
    arg_prefix: string | null;
    scope: string;
  }

  interface Props {
    /** Fires once the list of pending proposals reaches zero. */
    oncomplete: () => void;
  }

  const { oncomplete }: Props = $props();

  let pending = $state<ProposedRuleView[]>([]);
  let loading = $state(true);
  let busy = $state<Record<number, "approving" | "dismissing" | undefined>>({});

  async function refresh(): Promise<void> {
    try {
      pending = await invoke<ProposedRuleView[]>("list_proposed_permission_rules");
    } catch (err) {
      addToast(
        err instanceof Error ? err.message : String(err),
        "error",
      );
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

  async function approve(rule: ProposedRuleView): Promise<void> {
    busy = { ...busy, [rule.index]: "approving" };
    try {
      await invoke("apply_proposed_permission_rule", { index: rule.index });
      addToast($t("onboarding_permissions.rule_applied"), "success");
    } catch (err) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      busy = { ...busy, [rule.index]: undefined };
      await refresh();
    }
  }

  async function dismiss(rule: ProposedRuleView): Promise<void> {
    busy = { ...busy, [rule.index]: "dismissing" };
    try {
      await invoke("dismiss_proposed_permission_rule", { index: rule.index });
    } catch (err) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      busy = { ...busy, [rule.index]: undefined };
      await refresh();
    }
  }

  function actionPill(action: string): { label: string; cls: string } {
    if (action === "allow") {
      return {
        label: $t("onboarding_permissions.action_allow"),
        cls: "bg-emerald-500/15 text-emerald-600 dark:text-emerald-400 border-emerald-500/30",
      };
    }
    if (action === "deny") {
      return {
        label: $t("onboarding_permissions.action_deny"),
        cls: "bg-destructive/15 text-destructive border-destructive/30",
      };
    }
    return { label: action, cls: "bg-muted text-muted-foreground border-border" };
  }
</script>

<div class="permission-step" data-testid="onboarding-permission-step">
  <header class="step-header">
    <div class="step-icon">
      <ShieldCheck size={18} strokeWidth={1.75} aria-hidden="true" />
    </div>
    <div class="step-text">
      <p class="step-title">{$t("onboarding_permissions.title")}</p>
      <!-- subtitle includes a single <strong>; rendering as html is safe (no user input) -->
      <p class="step-subtitle">
        {@html $t("onboarding_permissions.subtitle_html")}
      </p>
    </div>
  </header>

  {#if loading}
    <div class="empty">
      <Loader2 class="anim-spin" size={16} /> {$t("onboarding_permissions.loading")}
    </div>
  {:else if pending.length === 0}
    <div class="empty">{$t("onboarding_permissions.empty")}</div>
  {:else}
    <ul class="rule-list">
      {#each pending as rule (rule.index)}
        {@const pill = actionPill(rule.action)}
        <li class="rule-card">
          <div class="rule-meta">
            <span class={`pill ${pill.cls}`}>
              {#if rule.action === "deny"}
                <ShieldX size={12} aria-hidden="true" />
              {:else}
                <ShieldCheck size={12} aria-hidden="true" />
              {/if}
              {pill.label}
            </span>
            <span class="rule-tool" title={$t("onboarding_permissions.tool_title")}>{rule.tool_name}</span>
            {#if rule.arg_prefix}
              <code class="rule-prefix" title={$t("onboarding_permissions.prefix_title")}>{rule.arg_prefix}</code>
            {/if}
            <span class="rule-scope" title={$t("onboarding_permissions.scope_title")}>
              {$t("onboarding_permissions.scope_label", { values: { scope: rule.scope } })}
            </span>
          </div>
          <div class="rule-actions">
            <Button
              size="sm"
              variant="outline"
              disabled={busy[rule.index] !== undefined}
              onclick={() => dismiss(rule)}
              data-testid={`onboarding-rule-dismiss-${rule.index}`}
            >
              {#if busy[rule.index] === "dismissing"}
                <Loader2 size={14} class="anim-spin" />
              {:else}
                <X size={14} />
              {/if}
              {$t("onboarding_permissions.deny")}
            </Button>
            <Button
              size="sm"
              variant="primary-gradient"
              disabled={busy[rule.index] !== undefined}
              onclick={() => approve(rule)}
              data-testid={`onboarding-rule-approve-${rule.index}`}
            >
              {#if busy[rule.index] === "approving"}
                <Loader2 size={14} class="anim-spin" />
              {:else}
                <Check size={14} />
              {/if}
              {$t("onboarding_permissions.approve")}
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
    gap: 0.75rem;
    padding: 0.75rem 1rem 1rem 1rem;
    border-top: 1px solid hsl(var(--border) / 0.6);
    background: hsl(var(--muted) / 0.25);
    flex-shrink: 0;
    max-height: 50%;
    overflow-y: auto;
  }

  .step-header {
    display: flex;
    gap: 0.625rem;
    align-items: flex-start;
  }

  .step-icon {
    width: 2rem;
    height: 2rem;
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
    font-size: 0.875rem;
    font-weight: 600;
    color: hsl(var(--foreground));
  }

  .step-subtitle {
    margin: 0.125rem 0 0 0;
    font-size: 0.75rem;
    line-height: 1.4;
    color: hsl(var(--muted-foreground));
  }

  .rule-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .rule-card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.625rem 0.75rem;
    border-radius: 0.5rem;
    border: 1px solid hsl(var(--border) / 0.6);
    background: hsl(var(--card));
  }

  .rule-meta {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex: 1;
    min-width: 0;
    flex-wrap: wrap;
  }

  .pill {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    padding: 0.125rem 0.5rem;
    border-radius: 999px;
    font-size: 0.6875rem;
    font-weight: 600;
    border-width: 1px;
    border-style: solid;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .rule-tool {
    font-family:
      ui-monospace, SFMono-Regular, "Menlo", "Monaco", "Consolas", "Liberation Mono",
      "Courier New", monospace;
    font-size: 0.75rem;
    font-weight: 600;
    color: hsl(var(--foreground));
  }

  .rule-prefix {
    padding: 0.0625rem 0.375rem;
    border-radius: 0.25rem;
    background: hsl(var(--muted) / 0.6);
    font-size: 0.7rem;
    color: hsl(var(--muted-foreground));
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 16rem;
  }

  .rule-scope {
    font-size: 0.6875rem;
    color: hsl(var(--muted-foreground));
  }

  .rule-actions {
    display: flex;
    align-items: center;
    gap: 0.375rem;
    flex-shrink: 0;
  }

  .empty {
    padding: 0.75rem;
    text-align: center;
    color: hsl(var(--muted-foreground));
    font-size: 0.75rem;
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
