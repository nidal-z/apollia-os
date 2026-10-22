<script lang="ts">
  /**
   * WhoCanUse - the step of an addition that says which agents may use the
   * connection: Apollia, the agent of the Chat, checked; every other agent of
   * this machine, to check. The checked agents get every tool of the
   * connection; a tool that writes, sends or runs code, and one nobody
   * classified, still asks for approval at each call.
   */
  import { t } from "svelte-i18n";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { Spinner } from "$lib/components/ui/progress";
  import { whoChoices, type WhoChoice } from "$lib/connections/addFlow";
  import type { AgentListItem } from "$lib/types";

  interface Props {
    agents: readonly AgentListItem[];
    loading: boolean;
    /** The agents checked, by id. */
    checked: string[];
    disabled?: boolean;
  }

  let { agents, loading, checked = $bindable(), disabled = false }: Props = $props();

  const choices = $derived<WhoChoice[]>(whoChoices(agents, $t("connections.add_flow.apollia_name")));

  function toggle(id: string, on: boolean): void {
    checked = on ? [...checked.filter((c) => c !== id), id] : checked.filter((c) => c !== id);
  }
</script>

<section class="space-y-3" data-testid="add-flow-who">
  <div>
    <h3 class="text-sm font-semibold text-foreground">{$t("connections.add_flow.who_title")}</h3>
    <p class="mt-0.5 text-caption text-muted-foreground">{$t("connections.add_flow.who_hint")}</p>
  </div>
  <ul class="space-y-1.5">
    {#each choices as choice (choice.id)}
      <li>
        <label
          class="flex cursor-pointer items-start gap-2.5 rounded-lg border border-border bg-card px-3 py-2.5"
          data-testid="add-flow-who-agent"
          data-agent={choice.id}
        >
          <Checkbox
            checked={checked.includes(choice.id)}
            onchange={(on: boolean) => toggle(choice.id, on)}
            class="mt-0.5"
            {disabled}
          />
          <span class="min-w-0">
            <span class="block text-body-sm font-medium text-foreground">{choice.name}</span>
            {#if choice.apollia}
              <span class="block text-caption text-muted-foreground">{$t("connections.add_flow.apollia_hint")}</span>
            {/if}
          </span>
        </label>
      </li>
    {/each}
  </ul>
  {#if loading}
    <p class="flex items-center gap-2 text-caption text-muted-foreground">
      <Spinner size={12} />
      {$t("connections.add_flow.who_loading")}
    </p>
  {:else if choices.length === 1}
    <p class="text-caption text-muted-foreground">{$t("connections.add_flow.who_no_other")}</p>
  {/if}
  <p class="text-caption text-muted-foreground">{$t("connections.add_flow.who_rule")}</p>
</section>
