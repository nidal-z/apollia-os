<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { RotateCw } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Select } from "$lib/components/ui/select";
  import { Dialog, DialogFooter } from "$lib/components/ui/dialog";
  import { ErrorBanner } from "$lib/components/operator";
  import SettingSectionSkeleton from "../../components/settings/SettingSectionSkeleton.svelte";
  import PermissionRuleCard from "../../components/settings/PermissionRuleCard.svelte";
  import {
    permissionRules,
    auditEntries,
    loadingRules,
    loadingAudit,
    rulesError,
    auditError,
    filterScope,
    filterTool,
    loadRules,
    loadAudit,
    revokeRule,
    revokeAll,
    setScopeFilter,
    setToolFilter,
    chatPermissionRules,
    loadingChatRules,
    chatRulesError,
    loadChatRules,
    deleteChatRule,
    sessionAuthorizations,
    loadingSessionAuths,
    sessionAuthsError,
    loadSessionAuthorizations,
    revokeSessionAuthorization,
    type PermissionRuleDto,
    type PermissionRuleScope,
    type SessionAuthorizationDto,
  } from "$lib/stores/permissions";
  import { addToast } from "$lib/components/ui/toast";

  type RevokeAllScope = "project" | "agent" | "global" | "all";

  const BULK_LABEL_KEYS: Record<RevokeAllScope, string> = {
    project: "settings.permissions.scope_project",
    agent: "settings.permissions.scope_agent",
    global: "settings.permissions.scope_global",
    all: "settings.permissions.scope_all",
  };

  let initialized = $state(false);
  let revokingId = $state<number | null>(null);
  let revokingChatId = $state<number | null>(null);
  let revokingSessionAuthKey = $state<string | null>(null);
  let bulkOpen = $state(false);
  let bulkScope = $state<RevokeAllScope>("project");
  let bulkSubmitting = $state(false);

  onMount(() => {
    void Promise.all([
      loadRules(),
      loadAudit(),
      loadChatRules(),
      loadSessionAuthorizations(),
    ]).finally(() => {
      initialized = true;
    });
  });

  async function handleSessionAuthRevoke(
    entry: SessionAuthorizationDto,
  ): Promise<void> {
    const key = `${entry.session_id}::${entry.tool_name}`;
    revokingSessionAuthKey = key;
    try {
      await revokeSessionAuthorization(entry.session_id, entry.tool_name);
      addToast(
        $t("settings.permissions.toast_session_revoked", {
          values: { tool: entry.tool_name },
        }),
        "success",
        { "data-testid": "session-auth-revoke-toast" },
      );
    } catch (err) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      revokingSessionAuthKey = null;
    }
  }

  async function handleChatRevoke(rule: PermissionRuleDto): Promise<void> {
    revokingChatId = rule.id;
    try {
      await deleteChatRule(rule.id);
      addToast(
        $t("settings.permissions.toast_chat_deleted", {
          values: { tool: rule.tool_name },
        }),
        "success",
        {
          "data-testid": "chat-permission-revoke-toast",
        },
      );
    } catch (err) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      revokingChatId = null;
    }
  }

  const toolOptions = $derived.by(() => {
    const names = new Set<string>();
    for (const r of $permissionRules) names.add(r.tool_name);
    return Array.from(names).sort();
  });

  function scopeFromValue(value: string): PermissionRuleScope | null {
    if (
      value === "session" ||
      value === "project" ||
      value === "agent" ||
      value === "global"
    )
      return value;
    return null;
  }

  async function handleScopeFilter(event: Event): Promise<void> {
    const value = (event.currentTarget as HTMLSelectElement).value;
    await setScopeFilter(scopeFromValue(value));
  }

  async function handleToolFilter(event: Event): Promise<void> {
    const value = (event.currentTarget as HTMLSelectElement).value;
    await setToolFilter(value === "" ? null : value);
  }

  async function handleRevoke(rule: PermissionRuleDto): Promise<void> {
    revokingId = rule.id;
    try {
      await revokeRule(rule.id);
      addToast(
        $t("settings.permissions.toast_rule_revoked", {
          values: { tool: rule.tool_name },
        }),
        "success",
        {
          "data-testid": "permission-revoke-toast",
        },
      );
    } catch (err) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      revokingId = null;
    }
  }

  function bulkScopeFilter(): PermissionRuleScope | null {
    if (bulkScope === "all") return null;
    return bulkScope;
  }

  const bulkCount = $derived.by(() => {
    const target = bulkScope === "all" ? null : bulkScope;
    if (target === null) return $permissionRules.length;
    return $permissionRules.filter((r) => r.scope === target).length;
  });

  function openBulk(): void {
    bulkScope = "project";
    bulkOpen = true;
  }

  function closeBulk(): void {
    if (bulkSubmitting) return;
    bulkOpen = false;
  }

  async function confirmBulk(): Promise<void> {
    bulkSubmitting = true;
    try {
      const removed = await revokeAll(bulkScopeFilter());
      addToast(
        $t("settings.permissions.toast_rules_revoked", {
          values: { count: removed },
        }),
        "success",
        {
          "data-testid": "permission-revoke-all-toast",
        },
      );
      bulkOpen = false;
    } catch (err) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      bulkSubmitting = false;
    }
  }

  async function reload(): Promise<void> {
    await Promise.all([
      loadRules(),
      loadAudit(),
      loadChatRules(),
      loadSessionAuthorizations(),
    ]);
    addToast($t("settings.permissions.toast_reloaded"), "info", {
      "data-testid": "permissions-reloaded-toast",
    });
  }

  function modeLabel(mode: string): string {
    switch (mode) {
      case "libre":
        return "Apollia Chat";
      case "agent":
        return "Agent";
      case "companion":
        return "Companion";
      default:
        return mode;
    }
  }

  function formatAuditDate(value: string): string {
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return value;
    return date.toISOString().slice(11, 16);
  }
</script>

<section class="space-y-4" data-testid="settings-permissions">
  <header class="flex items-center justify-between gap-3">
    <div>
      <h2 class="text-lg font-semibold">{$t("settings.permissions.title")}</h2>
      <p class="text-xs text-muted-foreground">
        {$t("settings.permissions.subtitle")}
      </p>
    </div>
    <div class="flex items-center gap-2">
      <Button
        variant="destructive"
        size="sm"
        onclick={openBulk}
        disabled={$permissionRules.length === 0}
        data-testid="permissions-revoke-all"
      >
        {$t("settings.permissions.revoke_all")}
      </Button>
      <Button
        variant="outline"
        size="sm"
        onclick={reload}
        loading={($loadingRules || $loadingAudit) && initialized}
        data-testid="permissions-reload"
      >
        <RotateCw size={14} class="mr-1" aria-hidden="true" />
        {$t("settings.permissions.reload")}
      </Button>
    </div>
  </header>

  <div class="grid gap-4 lg:grid-cols-[14rem_1fr]">
    <aside class="space-y-4 rounded-md border border-border bg-muted/30 p-3">
      <div class="space-y-1.5">
        <label class="text-[11px] uppercase tracking-wide text-muted-foreground" for="permissions-filter-scope">
          {$t("settings.permissions.filter_scope_label")}
        </label>
        <Select
          id="permissions-filter-scope"
          value={$filterScope ?? ""}
          onchange={handleScopeFilter}
          data-testid="permissions-filter-scope"
        >
          <option value="">{$t("settings.permissions.filter_scope_all")}</option>
          <option value="project">{$t("settings.permissions.scope_project")}</option>
          <option value="agent">{$t("settings.permissions.scope_agent")}</option>
          <option value="global">{$t("settings.permissions.scope_global")}</option>
        </Select>
      </div>

      <div class="space-y-1.5">
        <label class="text-[11px] uppercase tracking-wide text-muted-foreground" for="permissions-filter-tool">
          {$t("settings.permissions.filter_tool_label")}
        </label>
        <Select
          id="permissions-filter-tool"
          value={$filterTool ?? ""}
          onchange={handleToolFilter}
          data-testid="permissions-filter-tool"
        >
          <option value="">{$t("settings.permissions.filter_tool_all")}</option>
          {#each toolOptions as name (name)}
            <option value={name}>{name}</option>
          {/each}
        </Select>
      </div>
    </aside>

    <div class="space-y-3">
      {#if !initialized && $loadingRules}
        <SettingSectionSkeleton />
      {:else if $rulesError}
        <ErrorBanner message={$rulesError} data-testid="permissions-error" />
      {:else if $permissionRules.length === 0}
        <p class="rounded-md border border-dashed border-border px-4 py-6 text-center text-sm text-muted-foreground" data-testid="permissions-empty">
          {$t("settings.permissions.rules_empty")}
        </p>
      {:else}
        <p class="text-xs text-muted-foreground" data-testid="permissions-count">
          {$t("settings.permissions.rules_count", {
            values: { count: $permissionRules.length },
          })}
        </p>
        <ul class="space-y-2" data-testid="permission-rules-list">
          {#each $permissionRules as rule (rule.id)}
            <li>
              <PermissionRuleCard
                {rule}
                busy={revokingId === rule.id}
                onRevoke={handleRevoke}
              />
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  </div>

  <section class="space-y-2" data-testid="permissions-session-auths">
    <header class="flex items-center justify-between">
      <div>
        <h3 class="text-sm font-semibold">{$t("settings.permissions.sessions_title")}</h3>
        <p class="text-[11px] text-muted-foreground">
          {$t("settings.permissions.sessions_subtitle")}
        </p>
      </div>
      <span class="text-[11px] text-muted-foreground">
        {$t("settings.permissions.sessions_count", {
          values: { count: $sessionAuthorizations.length },
        })}
      </span>
    </header>

    {#if $sessionAuthsError}
      <ErrorBanner message={$sessionAuthsError} data-testid="session-auths-error" />
    {:else if !initialized && $loadingSessionAuths}
      <SettingSectionSkeleton />
    {:else if $sessionAuthorizations.length === 0}
      <p
        class="rounded-md border border-dashed border-border px-4 py-4 text-center text-xs text-muted-foreground"
        data-testid="session-auths-empty"
      >
        {$t("settings.permissions.sessions_empty")}
      </p>
    {:else}
      <ul
        class="space-y-2"
        data-testid="session-auths-list"
      >
        {#each $sessionAuthorizations as entry (entry.session_id + "::" + entry.tool_name)}
          {@const key = entry.session_id + "::" + entry.tool_name}
          <li
            class="flex items-start justify-between gap-3 rounded-md border border-border bg-card px-4 py-3 shadow-sm"
            data-testid="session-auth-row"
            data-session-id={entry.session_id}
            data-tool-name={entry.tool_name}
          >
            <div class="min-w-0 flex-1">
              <div class="flex flex-wrap items-center gap-2">
                <code class="font-mono text-[12.5px] text-foreground">{entry.tool_name}</code>
                <span
                  class="rounded bg-warning/10 px-1.5 py-px text-[10px] font-medium text-warning"
                >
                  {$t("settings.permissions.session_badge")}
                </span>
              </div>
              <div class="mt-0.5 text-[11px] text-muted-foreground">
                {modeLabel(entry.mode)} · {entry.session_title ?? entry.session_id.slice(0, 8)}
              </div>
              <div class="mt-0.5 text-[10.5px] text-muted-foreground italic">
                {$t("settings.permissions.session_expire_hint")}
              </div>
            </div>
            <Button
              variant="ghost"
              size="sm"
              class="h-7 px-2 text-[11px]"
              disabled={revokingSessionAuthKey === key}
              onclick={() => handleSessionAuthRevoke(entry)}
              data-testid="session-auth-revoke"
            >
              {$t("permissions.rules.revoke")}
            </Button>
          </li>
        {/each}
      </ul>
    {/if}
  </section>

  <section class="space-y-2" data-testid="permissions-chat">
    <header class="flex items-center justify-between">
      <div>
        <h3 class="text-sm font-semibold">Apollia Chat</h3>
        <p class="text-[11px] text-muted-foreground">
          {$t("settings.permissions.chat_subtitle")}
        </p>
      </div>
      <span class="text-[11px] text-muted-foreground">
        {$t("settings.permissions.chat_count", {
          values: { count: $chatPermissionRules.length },
        })}
      </span>
    </header>

    {#if $chatRulesError}
      <ErrorBanner message={$chatRulesError} data-testid="chat-permissions-error" />
    {:else if !initialized && $loadingChatRules}
      <SettingSectionSkeleton />
    {:else if $chatPermissionRules.length === 0}
      <p
        class="rounded-md border border-dashed border-border px-4 py-4 text-center text-xs text-muted-foreground"
        data-testid="chat-permissions-empty"
      >
        {$t("settings.permissions.chat_empty")}
      </p>
    {:else}
      <ul class="space-y-2" data-testid="chat-permission-rules-list">
        {#each $chatPermissionRules as rule (rule.id)}
          <li>
            <PermissionRuleCard
              {rule}
              busy={revokingChatId === rule.id}
              onRevoke={handleChatRevoke}
            />
          </li>
        {/each}
      </ul>
    {/if}
  </section>

  <section class="space-y-2" data-testid="permissions-audit">
    <header class="flex items-center justify-between">
      <h3 class="text-sm font-semibold">{$t("settings.permissions.audit_title")}</h3>
      <span class="text-[11px] text-muted-foreground">{$t("settings.permissions.audit_readonly")}</span>
    </header>

    {#if $auditError}
      <ErrorBanner message={$auditError} />
    {:else if $auditEntries.length === 0}
      <p class="text-xs text-muted-foreground">{$t("settings.permissions.audit_empty")}</p>
    {:else}
      <ul class="divide-y divide-border rounded-md border border-border text-xs">
        {#each $auditEntries.slice(0, 20) as entry (entry.id)}
          <li class="grid grid-cols-[3rem_8rem_1fr_8rem] items-center gap-2 px-3 py-1.5">
            <span class="font-mono text-muted-foreground">{formatAuditDate(entry.decided_at)}</span>
            <span class="font-medium">{entry.tool_name}</span>
            <span class="text-muted-foreground">
              {entry.decision}{#if entry.scope} · {entry.scope}{/if}{#if entry.rule_id} · {$t("settings.permissions.audit_rule_ref", { values: { id: entry.rule_id } })}{/if}
            </span>
            <span class="text-right text-muted-foreground">{entry.agent ?? "-"}</span>
          </li>
        {/each}
      </ul>
    {/if}
  </section>
</section>

<Dialog
  open={bulkOpen}
  onclose={closeBulk}
  size="sm"
  title={$t("settings.permissions.revoke_all")}
  data-testid="permissions-revoke-all-dialog"
>
  <div class="space-y-3 text-sm">
    <p class="text-muted-foreground">
      {$t("settings.permissions.bulk_intro")}
    </p>

    <div class="space-y-1.5">
      <label class="text-[11px] uppercase tracking-wide text-muted-foreground" for="permissions-revoke-all-scope">
        {$t("settings.permissions.filter_scope_label")}
      </label>
      <Select
        id="permissions-revoke-all-scope"
        value={bulkScope}
        onchange={(e) => {
          bulkScope = (e.currentTarget as HTMLSelectElement).value as RevokeAllScope;
        }}
        disabled={bulkSubmitting}
        data-testid="permissions-revoke-all-scope-select"
      >
        <option value="project">{$t("settings.permissions.scope_project")}</option>
        <option value="agent">{$t("settings.permissions.scope_agent")}</option>
        <option value="global">{$t("settings.permissions.scope_global")}</option>
        <option value="all">{$t("settings.permissions.scope_all")}</option>
      </Select>
    </div>

    <p class="rounded-md bg-muted/40 px-3 py-2 text-[12px]">
      <strong>{$t(BULK_LABEL_KEYS[bulkScope])}</strong> · {$t(
        "settings.permissions.bulk_summary",
        { values: { count: bulkCount } },
      )}
    </p>
  </div>

  <DialogFooter>
    <Button
      variant="ghost"
      size="sm"
      onclick={closeBulk}
      disabled={bulkSubmitting}
    >
      {$t("common.cancel")}
    </Button>
    <Button
      variant="destructive"
      size="sm"
      onclick={confirmBulk}
      loading={bulkSubmitting}
      data-testid="permissions-revoke-all-confirm"
    >
      {$t("permissions.rules.revoke")}
    </Button>
  </DialogFooter>
</Dialog>
