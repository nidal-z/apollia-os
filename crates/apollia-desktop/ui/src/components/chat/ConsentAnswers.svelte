<script lang="ts" module>
  /**
   * The three answers to a consent request, and nothing else: no scope to
   * pick, no word to type. "Always" holds for the asking agent and the action
   * family of the tool; the runtime decides what that covers.
   */
  export type ConsentAnswer = "once" | "always" | "refuse";

  /** The answers in the order the card shows them. */
  export const CONSENT_ANSWERS: readonly ConsentAnswer[] = ["once", "always", "refuse"];

  /** What `authorize_chat_tool` receives for one answer. */
  export interface ConsentDecision {
    decision: "accept" | "always_accept" | "refuse";
    reason: string | null;
  }

  /**
   * The decision sent for `answer`. The reason travels with a refusal only,
   * trimmed, and an empty one is no reason: refusing never waits on it.
   */
  export function consentDecision(answer: ConsentAnswer, reasonText: string): ConsentDecision {
    switch (answer) {
      case "once":
        return { decision: "accept", reason: null };
      case "always":
        return { decision: "always_accept", reason: null };
      case "refuse": {
        const reason = reasonText.trim();
        return { decision: "refuse", reason: reason.length > 0 ? reason : null };
      }
    }
  }
</script>

<script lang="ts">
  import { t } from "svelte-i18n";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";

  interface Props {
    /** Prefix of every `data-testid`, completed by `-{answer}-{toolName}`. */
    testidPrefix: string;
    toolName: string;
    disabled?: boolean;
    onanswer: (decision: ConsentDecision) => void;
  }

  let { testidPrefix, toolName, disabled = false, onanswer }: Props = $props();

  let reasonText = $state("");

  function answer(choice: ConsentAnswer): void {
    onanswer(consentDecision(choice, reasonText));
  }
</script>

<p class="mt-3 text-micro text-muted-foreground">{$t("approval.always_hint")}</p>

<div class="mt-2 flex flex-wrap items-center gap-2">
  <Button
    variant="default"
    size="sm"
    class="h-7 px-3 text-caption"
    {disabled}
    onclick={() => answer("once")}
    data-testid="{testidPrefix}-accept-{toolName}"
  >
    {$t("approval.action.once")}
  </Button>
  <Button
    variant="outline"
    size="sm"
    class="h-7 px-3 text-caption"
    {disabled}
    onclick={() => answer("always")}
    data-testid="{testidPrefix}-always-{toolName}"
  >
    {$t("approval.action.always_for_agent")}
  </Button>
</div>

<form
  class="mt-2 flex items-center gap-2"
  onsubmit={(event) => {
    event.preventDefault();
    answer("refuse");
  }}
>
  <Input
    size="sm"
    class="h-7 flex-1 text-caption"
    bind:value={reasonText}
    {disabled}
    maxlength={500}
    placeholder={$t("approval.refuse_reason_placeholder")}
    aria-label={$t("approval.refuse_reason_placeholder")}
    data-testid="{testidPrefix}-reason-{toolName}"
  />
  <Button
    type="submit"
    variant="outline"
    size="sm"
    class="h-7 px-3 text-caption"
    {disabled}
    data-testid="{testidPrefix}-refuse-{toolName}"
  >
    {$t("approval.action.refuse")}
  </Button>
</form>
