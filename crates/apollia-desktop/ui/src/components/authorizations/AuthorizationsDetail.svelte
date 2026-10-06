<script lang="ts">
  /**
   * AuthorizationsDetail - what the selected row of the list holds: for an
   * agent, its accesses by family, its standing consents and its fine rules;
   * for "every agent", the fine rules that apply to all. Each standing consent
   * and each rule is withdrawn in one click.
   */
  import { t } from "svelte-i18n";
  import { Button } from "$lib/components/ui/button";
  import { Card } from "$lib/components/operator";
  import { developerMode } from "$lib/stores/developerMode";
  import type { FamilyConsent, PermissionRuleDto } from "$lib/ipc/permissions";
  import AlwaysRow from "./AlwaysRow.svelte";
  import FamilyRow from "./FamilyRow.svelte";
  import FineRuleRow from "./FineRuleRow.svelte";
  import {
    ALL_AGENTS,
    alwaysOf,
    consentKey,
    familyLines,
    rulesOf,
    type AgentEntry,
  } from "./authorizations";
  import type { Authorizations } from "./useAuthorizations.svelte";

  interface Props {
    auth: Authorizations;
    /** The agent selected, `null` for the rules for every agent. */
    agent: AgentEntry | null;
    onOpenSheet: (agent: AgentEntry) => void;
    onWithdrawAlways: (consent: FamilyConsent) => void;
    onUndoAlways: (consent: FamilyConsent) => void;
    onWithdrawRule: (rule: PermissionRuleDto) => void;
    onUndoRule: (rule: PermissionRuleDto) => void;
    onEditRule: (rule: PermissionRuleDto) => void;
    onDuplicateRule: (rule: PermissionRuleDto) => void;
  }

  let {
    auth,
    agent,
    onOpenSheet,
    onWithdrawAlways,
    onUndoAlways,
    onWithdrawRule,
    onUndoRule,
    onEditRule,
    onDuplicateRule,
  }: Props = $props();

  const selection = $derived(agent?.id ?? ALL_AGENTS);
  const always = $derived(agent ? alwaysOf(auth.consents, agent.id) : []);
  const inForce = $derived(always.filter((c) => !auth.withdrawn.consents.has(consentKey(c))));
  const families = $derived(
    agent ? familyLines(auth.groupsOf(agent.id), auth.policy, inForce) : [],
  );
  const rules = $derived(rulesOf(auth.rules, selection));
</script>

{#snippet ruleList()}
  {#if rules.length === 0}
    <p class="m-0 text-caption italic text-muted-foreground" data-testid="authorizations-rules-empty">
      {agent ? $t("authorizations.rules_empty_agent") : $t("authorizations.rules_empty_all")}
    </p>
  {:else}
    <ul class="m-0 list-none p-0" data-testid="authorizations-rules">
      {#each rules as rule (rule.id)}
        <FineRuleRow
          {rule}
          what={auth.describe(rule.tool_name)}
          family={auth.familyOf(rule.tool_name)}
          agentName={agent?.name ?? null}
          withdrawn={auth.withdrawn.rules.has(rule.id)}
          busy={auth.busy === `rule:${rule.id}`}
          onWithdraw={onWithdrawRule}
          onUndo={onUndoRule}
          onEdit={onEditRule}
          onDuplicate={onDuplicateRule}
        />
      {/each}
    </ul>
  {/if}
{/snippet}

<Card class="p-4" data-testid="authorizations-detail">
  <div data-testid="authorizations-detail-selection" data-selection={selection}>
    {#if agent}
      <div class="flex items-start justify-between gap-3 border-b border-border pb-3">
        <div class="min-w-0">
          <h2 class="m-0 text-body-lg font-semibold text-foreground">{agent.name}</h2>
          <p class="m-0 text-caption text-muted-foreground">{agent.description}</p>
        </div>
        <Button variant="link" size="sm" onclick={() => onOpenSheet(agent)} data-testid="authorizations-open-sheet">
          {$t("authorizations.open_sheet")}
        </Button>
      </div>

      <section class="pt-3" aria-label={$t("authorizations.families_title")}>
        <h3 class="m-0 text-caption font-normal text-muted-foreground">{$t("authorizations.families_title")}</h3>
        {#if families.length === 0}
          <p class="m-0 mt-1 text-caption italic text-muted-foreground" data-testid="authorizations-families-empty">
            {$t("authorizations.families_empty")}
          </p>
        {:else}
          <ul class="m-0 list-none p-0" data-testid="authorizations-families">
            {#each families as line (line.family)}
              <FamilyRow {line} />
            {/each}
          </ul>
        {/if}
      </section>

      <section class="pt-4" aria-label={$t("authorizations.always_title")}>
        <h3 class="m-0 text-caption font-normal text-muted-foreground">{$t("authorizations.always_title")}</h3>
        {#if always.length === 0}
          <p class="m-0 mt-1 text-caption italic text-muted-foreground" data-testid="authorizations-always-empty">
            {$t("authorizations.always_empty")}
          </p>
        {:else}
          <ul class="m-0 list-none p-0" data-testid="authorizations-always-list">
            {#each always as consent (consentKey(consent))}
              <AlwaysRow
                {consent}
                withdrawn={auth.withdrawn.consents.has(consentKey(consent))}
                busy={auth.busy === `always:${consentKey(consent)}`}
                onWithdraw={onWithdrawAlways}
                onUndo={onUndoAlways}
              />
            {/each}
          </ul>
        {/if}
      </section>

      <section class="pt-4" aria-label={$t("authorizations.rules_title")}>
        <h3 class="m-0 text-caption font-normal text-muted-foreground">{$t("authorizations.rules_title")}</h3>
        <div class="mt-1">{@render ruleList()}</div>
      </section>
    {:else}
      <div class="border-b border-border pb-3">
        <h2 class="m-0 text-body-lg font-semibold text-foreground">{$t("authorizations.all_agents_rules")}</h2>
        <p class="m-0 text-caption text-muted-foreground">
          {$developerMode ? $t("dev.authorizations.all_agents_legend") : $t("authorizations.all_agents_legend")}
        </p>
      </div>
      <div class="pt-3">{@render ruleList()}</div>
    {/if}
  </div>
</Card>
