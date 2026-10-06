<script lang="ts">
  /**
   * RevokeAllDialog - withdraw at once, for every agent or for one, the fine
   * rules, the standing consents, or both. Immediate and irreversible, so it
   * states how much it withdraws and asks for a confirmation; Cancel comes
   * first. The accesses granted by family are never touched.
   */
  import { t } from "svelte-i18n";
  import { AlertTriangle } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Dialog, DialogFooter } from "$lib/components/ui/dialog";
  import { RadioItem } from "$lib/components/ui/radio";
  import { Select } from "$lib/components/ui/select";
  import type { FamilyConsent, PermissionRuleDto } from "$lib/ipc/permissions";
  import {
    planIsEmpty,
    revokeAllPlan,
    type AgentEntry,
    type RevokeAllPlan,
    type RevokeWhat,
  } from "./authorizations";

  interface Props {
    /** The rules and the standing consents still in force. */
    rules: PermissionRuleDto[];
    consents: FamilyConsent[];
    agents: AgentEntry[];
    /** Agent proposed when a single one is targeted. */
    fallbackAgent: string;
    submitting: boolean;
    onClose: () => void;
    onConfirm: (plan: RevokeAllPlan) => void;
  }

  let { rules, consents, agents, fallbackAgent, submitting, onClose, onConfirm }: Props = $props();

  let what = $state<RevokeWhat>("both");
  let scope = $state<"all" | "agent">("all");
  // svelte-ignore state_referenced_locally
  let agentId = $state(fallbackAgent);

  const plan = $derived(revokeAllPlan(what, scope === "all" ? null : agentId, rules, consents));
  const target = $derived(
    scope === "all"
      ? $t("authorizations.scope_all_agents")
      : (agents.find((agent) => agent.id === agentId)?.name ?? agentId),
  );

  function close(): void {
    if (!submitting) onClose();
  }
</script>

<Dialog
  open
  onclose={close}
  size="sm"
  title={$t("authorizations.revoke_all")}
  data-testid="authorizations-revoke-all-dialog"
>
  <div class="space-y-4 text-body-sm">
    <p class="m-0 text-caption text-muted-foreground">{$t("authorizations.revoke_all_intro")}</p>

    <div class="space-y-1.5">
      <span class="text-caption text-muted-foreground">{$t("authorizations.revoke_all_what")}</span>
      <div class="space-y-2" role="radiogroup" aria-label={$t("authorizations.revoke_all_what")}>
        <RadioItem value="rules" checked={what === "rules"} onchange={() => (what = "rules")} data-testid="authorizations-revoke-all-what-rules">
          {$t("authorizations.revoke_all_rules_only")}
        </RadioItem>
        <RadioItem value="always" checked={what === "always"} onchange={() => (what = "always")} data-testid="authorizations-revoke-all-what-always">
          {$t("authorizations.revoke_all_always_only")}
        </RadioItem>
        <RadioItem value="both" checked={what === "both"} onchange={() => (what = "both")} data-testid="authorizations-revoke-all-what-both">
          {$t("authorizations.revoke_all_both")}
        </RadioItem>
      </div>
    </div>

    <div class="space-y-1.5">
      <span class="text-caption text-muted-foreground">{$t("authorizations.revoke_all_scope")}</span>
      <div class="space-y-2" role="radiogroup" aria-label={$t("authorizations.revoke_all_scope")}>
        <RadioItem value="all" checked={scope === "all"} onchange={() => (scope = "all")} data-testid="authorizations-revoke-all-scope-all">
          {$t("authorizations.scope_all_agents")}
        </RadioItem>
        <div class="flex flex-wrap items-center gap-3">
          <RadioItem value="agent" checked={scope === "agent"} onchange={() => (scope = "agent")} data-testid="authorizations-revoke-all-scope-agent">
            {$t("authorizations.scope_one_agent")}
          </RadioItem>
          <Select
            bind:value={agentId}
            disabled={scope !== "agent" || submitting}
            aria-label={$t("authorizations.scope_one_agent")}
            class="max-w-[14rem]"
            data-testid="authorizations-revoke-all-agent"
          >
            {#each agents as agent (agent.id)}
              <option value={agent.id}>{agent.name}</option>
            {/each}
          </Select>
        </div>
      </div>
    </div>

    <p
      class="m-0 flex items-center gap-2 rounded-md bg-muted/40 px-3 py-2 text-body-xs"
      data-testid="authorizations-revoke-all-summary"
    >
      <AlertTriangle size={14} class="shrink-0 text-warning-a11y" aria-hidden="true" />
      <span>
        {$t("authorizations.revoke_all_summary", {
          values: { target, rules: plan.ruleCount, always: plan.consents.length },
        })}
      </span>
    </p>
  </div>

  <DialogFooter>
    <Button variant="ghost" size="sm" onclick={close} disabled={submitting} data-testid="authorizations-revoke-all-cancel">
      {$t("common.cancel")}
    </Button>
    <Button
      variant="destructive"
      size="sm"
      loading={submitting}
      disabled={submitting || planIsEmpty(plan)}
      onclick={() => onConfirm(plan)}
      data-testid="authorizations-revoke-all-confirm"
    >
      {$t("authorizations.revoke_all")}
    </Button>
  </DialogFooter>
</Dialog>
