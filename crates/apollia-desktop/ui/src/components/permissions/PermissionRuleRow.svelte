<script lang="ts">
  /**
   * PermissionRuleRow - one permission rule as a premium, bordered row.
   *
   * Shield icon carries the allow/deny reading (success / destructive) so the
   * decision does not rely on badge colour alone. All labels flow through
   * `$t()`; the code-executor invariant is surfaced with a dedicated badge.
   * Right-click (or the Menu key) opens a context menu (duplicate / revoke);
   * Delete revokes the focused row. Roving focus is driven by the parent list.
   */
  import { t } from "svelte-i18n";
  import {
    ShieldCheck,
    ShieldX,
    Check,
    XCircle,
    Globe,
    Terminal,
    Trash2,
    Copy,
  } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { ContextMenu } from "$lib/components/ui/context-menu";
  import type { ActionMenuItem } from "$lib/components/ui/action-menu";
  import { isCodeExecutor } from "$lib/ipc/permissions";
  import type {
    PermissionRuleDto,
    PermissionRuleScope,
  } from "$lib/stores/permissions";

  interface Props {
    rule: PermissionRuleDto;
    busy?: boolean;
    onRevoke: (rule: PermissionRuleDto) => void;
    onDuplicate?: (rule: PermissionRuleDto) => void;
  }

  let { rule, busy = false, onRevoke, onDuplicate }: Props = $props();

  const SCOPE_KEY: Record<PermissionRuleScope, string> = {
    session: "settings.permissions.session_badge",
    project: "settings.permissions.scope_project",
    agent: "settings.permissions.scope_agent",
    global: "settings.permissions.scope_global",
  };

  const SCOPE_VARIANT: Record<
    PermissionRuleScope,
    "warning" | "info" | "neutral" | "primary"
  > = {
    session: "warning",
    project: "info",
    agent: "neutral",
    global: "primary",
  };

  const isAllow = $derived(rule.action === "allow");
  const executor = $derived(isCodeExecutor(rule.tool_name));

  function formatDate(value: string): string {
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return value;
    return date.toISOString().slice(0, 10);
  }

  const expiration = $derived.by(() => {
    if (rule.scope === "session")
      return $t("settings.permissions.session_expire_hint");
    if (!rule.expires_at) return $t("settings.permissions.rule_permanent");
    return $t("settings.permissions.rule_expires", {
      values: { date: formatDate(rule.expires_at) },
    });
  });

  const created = $derived(
    $t("settings.permissions.rule_created", {
      values: { date: formatDate(rule.created_at) },
    }),
  );

  const menuItems = $derived.by((): ActionMenuItem[] => {
    const items: ActionMenuItem[] = [];
    if (onDuplicate) {
      items.push({
        id: "duplicate",
        label: $t("settings.permissions.action_duplicate"),
        icon: Copy,
        onclick: () => onDuplicate?.(rule),
        testid: "permission-rule-duplicate",
      });
    }
    items.push({
      id: "revoke",
      label: $t("permissions.rules.revoke"),
      icon: Trash2,
      variant: "destructive",
      disabled: busy,
      onclick: () => onRevoke(rule),
      testid: "permission-rule-revoke-menu",
    });
    return items;
  });

  function onKeydown(event: KeyboardEvent): void {
    if ((event.key === "Delete" || event.key === "Backspace") && !busy) {
      event.preventDefault();
      onRevoke(rule);
    }
  }
</script>

<ContextMenu items={menuItems} data-testid="permission-rule-menu">
  <!-- Roving-focus composite: listNavigation manages tabindex; Delete revokes. -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    role="group"
    aria-label={rule.tool_name}
    tabindex="-1"
    onkeydown={onKeydown}
    data-testid="permission-rule-card"
    data-scope={rule.scope}
    data-rule-id={rule.id}
    class="flex items-start gap-3 rounded-lg border border-border bg-card px-3.5 py-3 shadow-sm outline-none transition-colors
      hover:border-primary/35 focus-visible:ring-2 focus-visible:ring-ring/60"
  >
    <span
      class="mt-px flex size-8 shrink-0 items-center justify-center rounded-md {isAllow
        ? 'bg-success/10 text-success'
        : 'bg-destructive/10 text-destructive'}"
    >
      {#if isAllow}
        <ShieldCheck size={16} aria-hidden="true" />
      {:else}
        <ShieldX size={16} aria-hidden="true" />
      {/if}
    </span>

    <div class="min-w-0 flex-1">
      <div class="flex flex-wrap items-center gap-2">
        <span class="font-mono text-body-xs font-semibold [overflow-wrap:anywhere]">
          {rule.tool_name}
        </span>
        <Badge variant={isAllow ? "success" : "danger"} data-testid="permission-rule-action">
          {#snippet icon()}
            {#if isAllow}<Check size={10} />{:else}<XCircle size={10} />{/if}
          {/snippet}
          {isAllow
            ? $t("settings.permissions.add.action_allow")
            : $t("settings.permissions.add.action_deny")}
        </Badge>
        <Badge variant={SCOPE_VARIANT[rule.scope]} data-testid="permission-rule-scope">
          {#snippet icon()}
            {#if rule.scope === "global"}<Globe size={10} />{/if}
          {/snippet}
          {$t(SCOPE_KEY[rule.scope])}
        </Badge>
        {#if executor}
          <Badge variant="warning" data-testid="permission-rule-executor">
            {#snippet icon()}<Terminal size={10} />{/snippet}
            {$t("settings.permissions.executor_badge")}
          </Badge>
        {/if}
      </div>

      <dl class="mt-1.5 space-y-0.5 text-caption text-muted-foreground">
        {#if rule.arg_prefix}
          <div>
            <span class="font-medium text-foreground">{$t("settings.permissions.rule_prefix")}</span>
            <code class="ml-1 rounded bg-muted px-1 py-px font-mono [overflow-wrap:anywhere]">{rule.arg_prefix}</code>
          </div>
        {:else}
          <div class="italic">
            {isAllow
              ? $t("settings.permissions.rule_all_allowed")
              : $t("settings.permissions.rule_all_denied")}
          </div>
        {/if}

        {#if rule.scope === "project" && rule.project_path}
          <div>
            <span class="font-medium text-foreground">{$t("settings.permissions.rule_project")}</span>
            <code class="ml-1 rounded bg-muted px-1 py-px font-mono [overflow-wrap:anywhere]">{rule.project_path}</code>
          </div>
        {/if}

        {#if rule.scope === "agent" && rule.agent_id}
          <div>
            <span class="font-medium text-foreground">{$t("settings.permissions.rule_agent")}</span>
            <code class="ml-1 rounded bg-muted px-1 py-px font-mono [overflow-wrap:anywhere]">{rule.agent_id}</code>
          </div>
        {/if}

        <div>{expiration} · {created}</div>

        {#if rule.created_by}
          <div>
            <span class="font-medium text-foreground">{$t("settings.permissions.rule_author")}</span>
            {rule.created_by}
          </div>
        {/if}
      </dl>
    </div>

    <div class="flex-none">
      <Button
        variant="ghost"
        size="sm"
        class="h-7 px-2 text-caption"
        disabled={busy}
        loading={busy}
        onclick={() => onRevoke(rule)}
        data-testid="permission-rule-revoke"
      >
        {#snippet icon()}<Trash2 size={12} aria-hidden="true" />{/snippet}
        {$t("permissions.rules.revoke")}
      </Button>
    </div>
  </div>
</ContextMenu>
