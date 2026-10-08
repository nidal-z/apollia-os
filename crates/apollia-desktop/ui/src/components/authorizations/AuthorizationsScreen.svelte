<script lang="ts">
  /**
   * AuthorizationsScreen - what every agent may do on this machine, read at
   * once: the organisation policy at the head, then a list (the rules for
   * every agent first, then one row per agent) and the detail of the selected
   * row. A standing consent or a fine rule is withdrawn in one click, with an
   * undo in its line; "Revoke all" withdraws in bulk after a confirmation.
   * Writing a fine rule belongs to the developer mode.
   */
  import { untrack } from "svelte";
  import { t } from "svelte-i18n";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { addToast } from "$lib/components/ui/toast";
  import { Card, ErrorBanner, PageLayout, SkeletonList } from "$lib/components/operator";
  import { reportError } from "$lib/errors/reportError";
  import { requestAgentSheet } from "$lib/stores/agentFocus";
  import { agents } from "$lib/stores/agents";
  import { developerMode } from "$lib/stores/developerMode";
  import { navigateTo } from "$lib/stores/navigation";
  import type { FamilyConsent, PermissionRuleDto } from "$lib/ipc/permissions";
  import { APOLLIA_AGENT_ID } from "../agents/sheet/agentSheet";
  import AuthorizationsDetail from "./AuthorizationsDetail.svelte";
  import PolicyPanel from "./PolicyPanel.svelte";
  import RevokeAllDialog from "./RevokeAllDialog.svelte";
  import RuleDialog from "./RuleDialog.svelte";
  import {
    ALL_AGENTS,
    FAMILY_LABEL,
    agentSummary,
    canCreateRule,
    consentKey,
    familyLines,
    matchesFilter,
    rulesOf,
    type AgentEntry,
    type RevokeAllPlan,
    type RuleDialogMode,
    type RuleDraft,
  } from "./authorizations";
  import { createAuthorizations } from "./useAuthorizations.svelte";

  const auth = createAuthorizations();

  let query = $state("");
  let selection = $state<string>(ALL_AGENTS);
  let ruleDialog = $state<{ mode: RuleDialogMode; original: PermissionRuleDto | null; seed: number } | null>(null);
  let revokeAllOpen = $state(false);
  let seed = 0;

  const installed = $derived($agents.filter((agent) => agent.agent_type !== "worker"));
  const installedKey = $derived(installed.map((agent) => agent.name).join("\n"));

  $effect(() => {
    void installedKey;
    void untrack(() => auth.load(installed, [APOLLIA_AGENT_ID]));
  });

  /** The built-in agent, the installed ones, then any agent only governance still knows. */
  const entries = $derived.by((): AgentEntry[] => {
    const known: AgentEntry[] = [
      {
        id: APOLLIA_AGENT_ID,
        name: $t("authorizations.apollia_name"),
        description: $t("authorizations.apollia_description"),
      },
      ...installed.map((agent) => ({
        id: agent.name,
        name: agent.name,
        description: agent.description ?? "",
      })),
    ];
    const ids = new Set(known.map((entry) => entry.id));
    const others = new Set<string>();
    for (const consent of auth.consents) if (!ids.has(consent.agent_id)) others.add(consent.agent_id);
    for (const rule of auth.rules) {
      if (rule.scope === "agent" && rule.agent_id && !ids.has(rule.agent_id)) others.add(rule.agent_id);
    }
    return [
      ...known,
      ...[...others].sort().map((id) => ({
        id,
        name: id,
        description: $t("authorizations.agent_not_installed"),
      })),
    ];
  });

  function wordsOf(entry: AgentEntry): string[] {
    const lines = familyLines(auth.groupsOf(entry.id), auth.policy, []);
    return [
      ...lines.map((line) => $t(FAMILY_LABEL[line.family])),
      ...lines.flatMap((line) => line.covers),
      ...rulesOf(auth.rules, entry.id).map((rule) => `${rule.tool_name} ${rule.arg_prefix ?? ""}`),
    ];
  }

  const shown = $derived(entries.filter((entry) => matchesFilter(query, entry, wordsOf(entry))));
  const selected = $derived(entries.find((entry) => entry.id === selection) ?? null);
  const globalRules = $derived(
    rulesOf(auth.rules, ALL_AGENTS).filter((rule) => !auth.withdrawn.rules.has(rule.id)).length,
  );
  const rulesInForce = $derived(auth.rules.filter((rule) => !auth.withdrawn.rules.has(rule.id)));
  const consentsInForce = $derived(
    auth.consents.filter((consent) => !auth.withdrawn.consents.has(consentKey(consent))),
  );
  const fallbackAgent = $derived(selected?.id ?? entries[0]?.id ?? APOLLIA_AGENT_ID);

  function summaryOf(entry: AgentEntry) {
    return agentSummary(
      entry.id,
      familyLines(auth.groupsOf(entry.id), auth.policy, []).length,
      auth.consents,
      auth.rules,
      auth.withdrawn,
    );
  }

  async function attempt(run: () => Promise<unknown>): Promise<void> {
    try {
      await run();
    } catch (err) {
      reportError(err, { surface: "toast", testid: "authorizations-error-toast" });
    }
  }

  function openRuleDialog(mode: RuleDialogMode, original: PermissionRuleDto | null): void {
    if (!canCreateRule($developerMode)) return;
    seed += 1;
    ruleDialog = { mode, original, seed };
    void auth.loadTools();
  }

  async function saveRule(draft: RuleDraft): Promise<void> {
    const open = ruleDialog;
    if (open === null) return;
    await attempt(async () => {
      await auth.saveRule(open.mode, draft, open.original);
      ruleDialog = null;
      addToast($t("dev.authorizations.rule_saved"), "success", {
        "data-testid": "authorizations-rule-saved-toast",
      });
    });
  }

  async function confirmRevokeAll(plan: RevokeAllPlan): Promise<void> {
    await attempt(async () => {
      const done = await auth.revokeAll(plan);
      revokeAllOpen = false;
      addToast(
        $t("authorizations.revoke_all_done", { values: { rules: done.rules, always: done.always } }),
        "success",
        { "data-testid": "authorizations-revoke-all-toast" },
      );
    });
  }

  function openSheet(agent: AgentEntry): void {
    requestAgentSheet(agent.id);
    navigateTo("agents");
  }

  const rowBase =
    "block w-full rounded-md border px-3 py-2 text-left outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring/60";
  const rowIdle = "border-transparent hover:bg-muted/40";
  const rowChosen = "border-primary/40 bg-primary/10";
</script>

<PageLayout
  title={$t("authorizations.title")}
  subtitle={$t("authorizations.subtitle")}
  class="space-y-4 pb-8 [&>*:not(:first-child)]:mx-8"
  data-testid="authorizations-page"
>
  <PolicyPanel summary={auth.policy} onOpen={() => navigateTo("policy")} />

  <div class="flex flex-wrap items-center gap-2">
    <Input
      bind:value={query}
      size="sm"
      class="min-w-[12rem] flex-1"
      placeholder={$t("authorizations.filter_placeholder")}
      aria-label={$t("authorizations.filter_placeholder")}
      data-testid="authorizations-filter"
    />
    {#if canCreateRule($developerMode)}
      <Button size="sm" onclick={() => openRuleDialog("add", null)} data-testid="authorizations-add-rule">
        {$t("dev.authorizations.add_rule")}
      </Button>
    {/if}
    <Button
      variant="outline"
      size="sm"
      disabled={auth.loading}
      onclick={() => (revokeAllOpen = true)}
      data-testid="authorizations-revoke-all"
    >
      {$t("authorizations.revoke_all")}
    </Button>
  </div>

  {#if auth.error}
    <ErrorBanner
      tone="danger"
      message={$t("authorizations.load_failed")}
      onretry={() => void auth.load(installed, [APOLLIA_AGENT_ID])}
      loading={auth.loading}
      data-testid="authorizations-load-error"
    />
  {:else if auth.loading && auth.rules.length === 0 && auth.consents.length === 0}
    <SkeletonList count={4} />
  {:else}
    <div class="grid items-start gap-6 lg:grid-cols-[18rem_minmax(0,1fr)]">
      <Card class="space-y-1 p-3" data-testid="authorizations-list">
        <div class="px-1 pb-1 text-caption text-muted-foreground">{$t("authorizations.list_all_title")}</div>
        <button
          type="button"
          class="{rowBase} {selection === ALL_AGENTS ? rowChosen : rowIdle}"
          aria-current={selection === ALL_AGENTS}
          onclick={() => (selection = ALL_AGENTS)}
          data-testid="authorizations-list-all"
        >
          <span class="block text-body-sm text-foreground">{$t("authorizations.all_agents_rules")}</span>
          <span class="block text-caption text-muted-foreground">
            {$t("authorizations.count_rules", { values: { count: globalRules } })}
          </span>
        </button>

        <div class="px-1 pb-1 pt-3 text-caption text-muted-foreground">{$t("authorizations.list_agents_title")}</div>
        {#each shown as entry (entry.id)}
          {@const summary = summaryOf(entry)}
          <button
            type="button"
            class="{rowBase} {selection === entry.id ? rowChosen : rowIdle}"
            aria-current={selection === entry.id}
            onclick={() => (selection = entry.id)}
            data-testid="authorizations-list-agent"
            data-agent={entry.id}
          >
            <span class="block truncate text-body-sm text-foreground">{entry.name}</span>
            <span class="block text-caption text-muted-foreground">
              {$t("authorizations.count_families", { values: { count: summary.families } })}
              · {$t("authorizations.count_always", { values: { count: summary.always } })}
              · {$t("authorizations.count_rules", { values: { count: summary.rules } })}
            </span>
          </button>
        {:else}
          <p class="m-0 px-1 text-caption italic text-muted-foreground" data-testid="authorizations-list-empty">
            {$t("authorizations.filter_empty")}
          </p>
        {/each}
      </Card>

      <AuthorizationsDetail
        {auth}
        agent={selected}
        onOpenSheet={openSheet}
        onWithdrawAlways={(consent: FamilyConsent) => void attempt(() => auth.withdrawAlways(consent))}
        onUndoAlways={(consent: FamilyConsent) => void attempt(() => auth.undoAlways(consent))}
        onWithdrawRule={(rule: PermissionRuleDto) => void attempt(() => auth.withdrawRule(rule))}
        onUndoRule={(rule: PermissionRuleDto) => void attempt(() => auth.undoRule(rule))}
        onEditRule={(rule: PermissionRuleDto) => openRuleDialog("edit", rule)}
        onDuplicateRule={(rule: PermissionRuleDto) => openRuleDialog("duplicate", rule)}
      />
    </div>
  {/if}

  <div
    class="flex flex-wrap items-center justify-between gap-3 rounded-lg bg-muted/50 px-4 py-3"
    data-testid="authorizations-builtin-tools"
  >
    <p class="m-0 min-w-0 flex-1 text-caption text-muted-foreground">{$t("authorizations.builtin_tools_note")}</p>
    <Button variant="link" size="sm" onclick={() => navigateTo("integrations")} data-testid="authorizations-open-connections">
      {$t("authorizations.open_connections")}
    </Button>
  </div>
</PageLayout>

{#if ruleDialog}
  {#key ruleDialog.seed}
    <RuleDialog
      mode={ruleDialog.mode}
      original={ruleDialog.original}
      tools={auth.tools}
      agents={entries}
      {fallbackAgent}
      saving={auth.busy === "rule:save"}
      onClose={() => (ruleDialog = null)}
      onSave={(draft) => void saveRule(draft)}
    />
  {/key}
{/if}

{#if revokeAllOpen}
  <RevokeAllDialog
    rules={rulesInForce}
    consents={consentsInForce}
    agents={entries}
    {fallbackAgent}
    submitting={auth.busy === "revoke-all"}
    onClose={() => (revokeAllOpen = false)}
    onConfirm={(plan) => void confirmRevokeAll(plan)}
  />
{/if}
