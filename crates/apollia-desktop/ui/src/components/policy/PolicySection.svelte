<script lang="ts">
  /**
   * PolicySection - one card of the policy screen: a title, a sentence saying
   * what this part of the policy is for, its rules, and optionally a folded
   * "Details" line for what only IT reads (the key fingerprint, the folder).
   */
  import type { Snippet } from "svelte";
  import { t } from "svelte-i18n";
  import { Card } from "$lib/components/ui/card";
  import { Disclosure } from "$lib/components/ui/disclosure";
  import type { PolicyRuleLine } from "$lib/policy/policyRules";
  import PolicyRule from "./PolicyRule.svelte";

  interface Props {
    /** Stable id, for the `data-testid` of the card. */
    id: string;
    title: string;
    description?: string;
    rules?: PolicyRuleLine[];
    /** Rules shown under the folded "Details" line. */
    details?: PolicyRuleLine[];
    /** Content above the rules, such as an alert line. */
    children?: Snippet;
  }

  let { id, title, description, rules = [], details = [], children }: Props = $props();
</script>

<Card surface="solid" class="rounded-lg p-5" data-testid="policy-section-{id}">
  <h2 class="text-base font-semibold text-foreground">{title}</h2>
  {#if description}
    <p class="mt-1 text-sm text-muted-foreground">{description}</p>
  {/if}
  {@render children?.()}
  {#if rules.length > 0}
    <dl class="mt-3 divide-y divide-border">
      {#each rules as rule (rule.id)}
        <PolicyRule {rule} />
      {/each}
    </dl>
  {/if}
  {#if details.length > 0}
    <div class="mt-2 border-t border-border pt-2">
      <Disclosure testid="policy-details-{id}">
        {#snippet summary()}
          <span class="text-sm text-muted-foreground">{$t("policy.details")}</span>
        {/snippet}
        <dl class="divide-y divide-border">
          {#each details as rule (rule.id)}
            <PolicyRule {rule} />
          {/each}
        </dl>
      </Disclosure>
    </div>
  {/if}
</Card>
