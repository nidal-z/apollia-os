<script lang="ts">
  /**
   * FineRuleRow - one fine rule in plain words: what it allows or refuses,
   * its family, whom it targets and since when, with its withdrawal in one
   * click. The developer mode adds its technical detail, and the two actions
   * that write a rule: change it and copy it. Once withdrawn the line stays
   * in place, says so, and offers the undo.
   */
  import { locale, t } from "svelte-i18n";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { developerMode } from "$lib/stores/developerMode";
  import type { PermissionRuleDto } from "$lib/ipc/permissions";
  import {
    FAMILY_LABEL,
    formatDay,
    ruleActions,
    ruleSentence,
    type FamilyKey,
  } from "./authorizations";

  interface Props {
    rule: PermissionRuleDto;
    /** What the tool does in plain words. */
    what: string;
    family: FamilyKey | "";
    /** Name of the agent the rule targets, `null` for every agent. */
    agentName: string | null;
    withdrawn: boolean;
    busy?: boolean;
    onWithdraw: (rule: PermissionRuleDto) => void;
    onUndo: (rule: PermissionRuleDto) => void;
    onEdit: (rule: PermissionRuleDto) => void;
    onDuplicate: (rule: PermissionRuleDto) => void;
  }

  let {
    rule,
    what,
    family,
    agentName,
    withdrawn,
    busy = false,
    onWithdraw,
    onUndo,
    onEdit,
    onDuplicate,
  }: Props = $props();

  const allows = $derived(rule.action === "allow");
  const sentence = $derived(ruleSentence(rule, what));
  const actions = $derived(ruleActions($developerMode));
  const meta = $derived(
    [
      family === "" ? null : $t(FAMILY_LABEL[family]),
      agentName ?? $t("authorizations.scope_all_agents"),
      $t("authorizations.rule_added", {
        values: { date: formatDay(rule.created_at, $locale ?? "en") },
      }),
    ]
      .filter((part): part is string => part !== null)
      .join(" · "),
  );
</script>

<li
  class="flex items-start gap-3 border-b border-border/40 py-2.5 last:border-b-0"
  data-testid="authorizations-rule"
  data-rule-id={rule.id}
  data-state={withdrawn ? "withdrawn" : "in-force"}
>
  <Badge
    size="sm"
    variant={allows ? "success" : "danger"}
    outline
    class="mt-0.5 shrink-0"
    data-testid="authorizations-rule-effect"
  >
    {allows ? $t("authorizations.effect_allows") : $t("authorizations.effect_refuses")}
  </Badge>
  <div class="min-w-0 flex-1">
    <div
      class="text-body-sm [overflow-wrap:anywhere] {withdrawn ? 'text-muted-foreground' : 'text-foreground'}"
    >
      {$t(sentence.key, { values: sentence.values })}
    </div>
    <div class="text-caption text-muted-foreground">
      {withdrawn ? $t("authorizations.rule_withdrawn") : meta}
    </div>
    {#if $developerMode && !withdrawn}
      <div
        class="mt-0.5 font-mono text-caption text-muted-foreground [overflow-wrap:anywhere]"
        data-testid="authorizations-rule-detail"
      >
        {rule.arg_prefix
          ? $t("dev.authorizations.rule_detail", {
              values: { prefix: rule.arg_prefix, tool: rule.tool_name, id: rule.id },
            })
          : $t("dev.authorizations.rule_detail_no_prefix", {
              values: { tool: rule.tool_name, id: rule.id },
            })}
      </div>
    {/if}
  </div>
  {#if withdrawn}
    <Badge size="sm" variant="neutral" outline>{$t("authorizations.withdrawn_badge")}</Badge>
    <Button
      variant="ghost"
      size="sm"
      disabled={busy}
      loading={busy}
      onclick={() => onUndo(rule)}
      data-testid="authorizations-rule-undo"
    >
      {$t("common.cancel")}
    </Button>
  {:else}
    <div class="flex shrink-0 flex-wrap items-center justify-end gap-1">
      {#if actions.includes("edit")}
        <Button
          variant="ghost"
          size="sm"
          disabled={busy}
          onclick={() => onEdit(rule)}
          data-testid="authorizations-rule-edit"
        >
          {$t("dev.authorizations.edit")}
        </Button>
      {/if}
      {#if actions.includes("duplicate")}
        <Button
          variant="ghost"
          size="sm"
          disabled={busy}
          onclick={() => onDuplicate(rule)}
          data-testid="authorizations-rule-duplicate"
        >
          {$t("dev.authorizations.duplicate")}
        </Button>
      {/if}
      <Button
        variant="outline"
        size="sm"
        disabled={busy}
        loading={busy}
        onclick={() => onWithdraw(rule)}
        data-testid="authorizations-rule-withdraw"
      >
        {$t("authorizations.withdraw")}
      </Button>
    </div>
  {/if}
</li>
