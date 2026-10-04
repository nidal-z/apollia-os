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
    /** The answers the request offers; all three when absent. */
    answers?: readonly ConsentAnswer[];
    /** The call runs code: "always" then allows this command for the agent,
     *  and without it every run asks. */
    runsCode?: boolean;
    /** The code chains several commands, which "always" never covers. */
    compound?: boolean;
    disabled?: boolean;
    onanswer: (decision: ConsentDecision) => void;
  }

  let {
    testidPrefix,
    toolName,
    answers = CONSENT_ANSWERS,
    runsCode = false,
    compound = false,
    disabled = false,
    onanswer,
  }: Props = $props();

  const offersAlways = $derived(answers.includes("always"));

  /** The line above the answers, saying what "always" covers, or why it is
   *  not offered on code. */
  const hintKey = $derived.by((): string | null => {
    if (runsCode) {
      if (offersAlways) return "approval.code.always_hint";
      return compound ? "approval.code.compound_hint" : "approval.code.each_time_hint";
    }
    return offersAlways ? "approval.always_hint" : null;
  });

  let reasonText = $state("");

  function answer(choice: ConsentAnswer): void {
    onanswer(consentDecision(choice, reasonText));
  }
</script>

{#if hintKey}
  <p class="mt-3 text-micro text-muted-foreground" data-testid="{testidPrefix}-hint-{toolName}">
    {$t(hintKey)}
  </p>
{/if}

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
  {#if offersAlways}
    <Button
      variant="outline"
      size="sm"
      class="h-7 px-3 text-caption"
      {disabled}
      onclick={() => answer("always")}
      data-testid="{testidPrefix}-always-{toolName}"
    >
      {runsCode ? $t("approval.code.always") : $t("approval.action.always_for_agent")}
    </Button>
  {/if}
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
