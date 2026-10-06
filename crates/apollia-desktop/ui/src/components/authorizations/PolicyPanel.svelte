<script lang="ts">
  /**
   * PolicyPanel - the organisation policy at the head of the authorizations:
   * none, active or refused, with what it imposes in one sentence and the way
   * to its own screen. Read only: the policy is written and signed by the
   * organisation, never here.
   */
  import { locale, t } from "svelte-i18n";
  import { KeyRound } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Card } from "$lib/components/operator";
  import { formatDay, imposesSomething, type PolicySummary } from "./authorizations";

  interface Props {
    summary: PolicySummary;
    onOpen: () => void;
  }

  let { summary, onOpen }: Props = $props();

  const STATE_LABEL: Record<PolicySummary["state"], string> = {
    none: "authorizations.policy_state_none",
    active: "authorizations.policy_state_active",
    invalid: "authorizations.policy_state_invalid",
  };
</script>

<Card class="flex flex-wrap items-center gap-3 p-4" data-testid="authorizations-policy">
  <span
    class="flex size-9 shrink-0 items-center justify-center rounded-md {summary.state === 'invalid'
      ? 'bg-destructive/10 text-destructive'
      : 'bg-muted text-muted-foreground'}"
  >
    <KeyRound size={16} aria-hidden="true" />
  </span>
  <div class="min-w-0 flex-1" data-testid="authorizations-policy-body" data-state={summary.state}>
    <div class="text-body-sm font-medium text-foreground">{$t("authorizations.policy_title")}</div>
    <div class="text-caption {summary.state === 'invalid' ? 'text-destructive' : 'text-muted-foreground'}">
      {#if summary.state === "none"}
        {$t("authorizations.policy_none")}
      {:else if summary.state === "invalid"}
        {$t("authorizations.policy_invalid")}
      {:else}
        {$t("authorizations.policy_active", {
          values: {
            publisher: summary.publisher ?? "",
            date: formatDay(summary.signedAt ?? "", $locale ?? "en"),
          },
        })}
        {#if imposesSomething(summary)}
          {$t("authorizations.policy_imposes", {
            values: {
              families: summary.deniedFamilies.length,
              connections: summary.deniedConnections.length,
              settings: summary.fixedSettings.length,
            },
          })}
        {/if}
      {/if}
    </div>
  </div>
  {#if summary.state === "active" && imposesSomething(summary)}
    <Badge size="sm" variant="warning" outline data-testid="authorizations-policy-imposed">
      {$t("settings.policy_locked")}
    </Badge>
  {/if}
  <Badge size="sm" variant={summary.state === "invalid" ? "danger" : "neutral"} outline>
    {$t(STATE_LABEL[summary.state])}
  </Badge>
  <Button variant="outline" size="sm" onclick={onOpen} data-testid="authorizations-policy-open">
    {summary.state === "invalid" ? $t("authorizations.policy_open_detail") : $t("authorizations.policy_open")}
  </Button>
</Card>
