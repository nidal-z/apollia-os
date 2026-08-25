<script lang="ts">
  /**
   * RulesSection - persistent permission rules: the add form, filter aside,
   * the rule list, and the bulk revoke-all dialog.
   *
   * The code-executor-never-blanket-authorized invariant is stated up front
   * (info banner) and enforced at creation by AddRuleForm. The header "Reload"
   * refreshes every section on the page, not just this one.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { flip } from "svelte/animate";
  import { fly } from "svelte/transition";
  import { RotateCw, Plus, Shield } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { ErrorBanner } from "$lib/components/operator";
  import { listNavigation } from "$lib/components/operator/listNavigation";
  import { listFlip, rowIn, rowOut } from "$lib/design/listMotion";
  import { addToast } from "$lib/components/ui/toast";
  import { reportError } from "$lib/errors/reportError";
  import SettingsSection from "../settings/SettingsSection.svelte";
  import SettingSectionSkeleton from "../settings/SettingSectionSkeleton.svelte";
  import PermissionRuleRow from "./PermissionRuleRow.svelte";
  import AddRuleForm from "./AddRuleForm.svelte";
  import PermissionErrorBanner from "./PermissionErrorBanner.svelte";
  import RuleFilters from "./RuleFilters.svelte";
  import RevokeAllDialog from "./RevokeAllDialog.svelte";
  import { governanceListTools } from "$lib/ipc/permissions";
  import {
    permissionRules,
    loadingRules,
    loadingAudit,
    rulesError,
    loadRules,
    loadAudit,
    loadChatRules,
    loadSessionAuthorizations,
    revokeRule,
    type PermissionRuleDto,
  } from "$lib/stores/permissions";

  interface AddRuleInitial {
    tool: string;
    action: "allow" | "deny";
    scope: "global" | "project";
    argPrefix: string;
    projectPath: string;
  }

  let initialized = $state(false);
  let revokingId = $state<number | null>(null);
  let availableTools = $state<string[]>([]);
  let addOpen = $state(false);
  let formSeed = $state(0);
  let addInitial = $state<AddRuleInitial | null>(null);
  let bulkOpen = $state(false);

  onMount(() => {
    void loadAvailableTools();
    void Promise.all([loadRules(), loadAudit()]).finally(() => (initialized = true));
  });

  async function loadAvailableTools(): Promise<void> {
    try {
      const tools = await governanceListTools();
      availableTools = tools.map((tool) => tool.name).sort();
    } catch {
      availableTools = [];
    }
  }

  function openAddForm(): void {
    addInitial = null;
    formSeed += 1;
    addOpen = true;
  }

  function duplicateRule(rule: PermissionRuleDto): void {
    addInitial = {
      tool: rule.tool_name,
      action: rule.action === "deny" ? "deny" : "allow",
      scope: rule.scope === "project" ? "project" : "global",
      argPrefix: rule.arg_prefix ?? "",
      projectPath: rule.project_path ?? "",
    };
    formSeed += 1;
    addOpen = true;
  }

  function onCreated(): void {
    addOpen = false;
    addInitial = null;
    void loadRules();
  }

  async function handleRevoke(rule: PermissionRuleDto): Promise<void> {
    revokingId = rule.id;
    try {
      await revokeRule(rule.id);
      addToast(
        $t("settings.permissions.toast_rule_revoked", { values: { tool: rule.tool_name } }),
        "success",
        { "data-testid": "permission-revoke-toast" },
      );
    } catch (err) {
      reportError(err, { surface: "toast" });
    } finally {
      revokingId = null;
    }
  }

  async function reload(): Promise<void> {
    await Promise.all([loadRules(), loadAudit(), loadChatRules(), loadSessionAuthorizations()]);
    addToast($t("settings.permissions.toast_reloaded"), "info", {
      "data-testid": "permissions-reloaded-toast",
    });
  }
</script>

<SettingsSection
  title={$t("settings.permissions.rules_title")}
  data-testid="permissions-rules-section"
>
  {#snippet icon()}<Shield size={15} strokeWidth={1.75} />{/snippet}
  {#snippet actions()}
    <Button variant="default" size="sm" onclick={openAddForm} data-testid="permission-rule-add-toggle">
      {#snippet icon()}<Plus size={14} aria-hidden="true" />{/snippet}
      {$t("settings.permissions.add.button")}
    </Button>
    <Button
      variant="destructive"
      size="sm"
      onclick={() => (bulkOpen = true)}
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
      {#snippet icon()}<RotateCw size={14} aria-hidden="true" />{/snippet}
      {$t("settings.permissions.reload")}
    </Button>
  {/snippet}

  <ErrorBanner tone="info" data-testid="permissions-executor-note">
    {$t("settings.permissions.executor_note")}
  </ErrorBanner>

  {#if addOpen}
    {#key formSeed}
      <AddRuleForm {availableTools} initial={addInitial} onCancel={() => (addOpen = false)} onCreated={onCreated} />
    {/key}
  {/if}

  <div class="grid gap-4 lg:grid-cols-[13rem_1fr]">
    <RuleFilters />

    <div class="min-w-0 space-y-3">
      {#if !initialized && $loadingRules}
        <SettingSectionSkeleton />
      {:else if $rulesError}
        <PermissionErrorBanner
          error={$rulesError}
          onretry={loadRules}
          loading={$loadingRules}
          data-testid="permissions-error"
        />
      {:else if $permissionRules.length === 0}
        <p
          class="rounded-lg border border-dashed border-border px-4 py-6 text-center text-body-sm text-muted-foreground"
          data-testid="permissions-empty"
        >
          {$t("settings.permissions.rules_empty")}
        </p>
      {:else}
        <p class="text-caption tabular-nums text-muted-foreground" data-testid="permissions-count">
          {$t("settings.permissions.rules_count", { values: { count: $permissionRules.length } })}
        </p>
        <ul
          class="space-y-2"
          role="list"
          data-testid="permission-rules-list"
          use:listNavigation={{ rowSelector: '[data-testid="permission-rule-card"]' }}
        >
          {#each $permissionRules as rule (rule.id)}
            <li animate:flip={listFlip()} in:fly={rowIn()} out:fly={rowOut()}>
              <PermissionRuleRow {rule} busy={revokingId === rule.id} onRevoke={handleRevoke} onDuplicate={duplicateRule} />
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  </div>
</SettingsSection>

<RevokeAllDialog open={bulkOpen} onClose={() => (bulkOpen = false)} />
