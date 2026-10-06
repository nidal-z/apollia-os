<script lang="ts">
  /**
   * RuleDialog - write a fine rule, in developer mode: a new one, a copy of
   * an existing one, or a change to it. A rule allows or refuses one tool,
   * narrowed by an argument prefix, for one agent or for every agent.
   * Allowing a tool that runs code needs a single simple command.
   */
  import { locale, t } from "svelte-i18n";
  import { Button } from "$lib/components/ui/button";
  import { Dialog, DialogFooter } from "$lib/components/ui/dialog";
  import { Input } from "$lib/components/ui/input";
  import { RadioItem } from "$lib/components/ui/radio";
  import { Select } from "$lib/components/ui/select";
  import { developerMode } from "$lib/stores/developerMode";
  import type { PermissionRuleDto } from "$lib/ipc/permissions";
  import {
    DRAFT_PROBLEM_LABEL,
    FAMILY_LABEL,
    FAMILY_ORDER,
    RULE_DIALOG_TITLE,
    draftFrom,
    draftProblem,
    formatDay,
    ruleSentence,
    type AgentEntry,
    type FamilyKey,
    type RuleDialogMode,
    type RuleDraft,
  } from "./authorizations";
  import type { RuleTool } from "./useAuthorizations.svelte";

  interface Props {
    mode: RuleDialogMode;
    /** The rule copied or changed, `null` for a new one. */
    original: PermissionRuleDto | null;
    tools: RuleTool[];
    agents: AgentEntry[];
    /** Agent proposed when the rule targets one. */
    fallbackAgent: string;
    saving: boolean;
    onClose: () => void;
    onSave: (draft: RuleDraft) => void;
  }

  let { mode, original, tools, agents, fallbackAgent, saving, onClose, onSave }: Props = $props();

  function familyOfTool(name: string): FamilyKey | "" {
    return tools.find((tool) => tool.name === name)?.family ?? "";
  }

  // The dialog is mounted anew for each rule, so seeding the draft from the
  // props once is intentional.
  // svelte-ignore state_referenced_locally
  let draft = $state<RuleDraft>(
    draftFrom(original, original ? familyOfTool(original.tool_name) : "", fallbackAgent),
  );
  let touched = $state(false);

  const offered = $derived(
    draft.family === "" ? tools : tools.filter((tool) => tool.family === draft.family),
  );
  const problem = $derived(draftProblem(draft));
  const origin = $derived.by(() => {
    if (original === null) return null;
    const sentence = ruleSentence(
      original,
      tools.find((tool) => tool.name === original.tool_name)?.description || original.tool_name,
    );
    const target =
      original.scope === "agent"
        ? (agents.find((agent) => agent.id === original.agent_id)?.name ?? original.agent_id ?? "")
        : $t("authorizations.scope_all_agents");
    return {
      id: original.id,
      what: $t(sentence.key, { values: sentence.values }),
      target,
      date: formatDay(original.created_at, $locale ?? "en"),
    };
  });

  function pickFamily(family: string): void {
    draft.family = (FAMILY_ORDER as readonly string[]).includes(family) ? (family as FamilyKey) : "";
    if (draft.family !== "" && familyOfTool(draft.tool) !== draft.family) draft.tool = "";
  }

  function submit(): void {
    touched = true;
    if (problem !== null || saving) return;
    onSave({ ...draft });
  }
</script>

{#if $developerMode}
  <Dialog open onclose={onClose} size="md" title={$t(RULE_DIALOG_TITLE[mode])} data-testid="authorizations-rule-dialog">
    <form
      class="space-y-4 text-body-sm"
      onsubmit={(event) => {
        event.preventDefault();
        submit();
      }}
    >
      <p class="m-0 text-caption text-muted-foreground" data-testid="authorizations-rule-dialog-intro">
        {#if origin && mode === "duplicate"}
          {$t("dev.authorizations.dialog_duplicate_intro", { values: origin })}
        {:else if origin && mode === "edit"}
          {$t("dev.authorizations.dialog_edit_intro", { values: origin })}
        {:else}
          {$t("dev.authorizations.dialog_add_intro")}
        {/if}
      </p>

      <div class="space-y-1.5">
        <span class="text-caption text-muted-foreground">{$t("dev.authorizations.field_effect")}</span>
        <div class="flex flex-wrap gap-4" role="radiogroup" aria-label={$t("dev.authorizations.field_effect")}>
          <RadioItem
            value="allow"
            checked={draft.action === "allow"}
            onchange={() => (draft.action = "allow")}
            data-testid="authorizations-rule-effect-allow"
          >
            {$t("dev.authorizations.effect_allow")}
          </RadioItem>
          <RadioItem
            value="deny"
            checked={draft.action === "deny"}
            onchange={() => (draft.action = "deny")}
            data-testid="authorizations-rule-effect-deny"
          >
            {$t("dev.authorizations.effect_deny")}
          </RadioItem>
        </div>
      </div>

      <div class="space-y-1.5">
        <label class="text-caption text-muted-foreground" for="authorizations-rule-family">
          {$t("dev.authorizations.field_family")}
        </label>
        <Select
          id="authorizations-rule-family"
          value={draft.family}
          onchange={(event) => pickFamily(event.currentTarget.value)}
          data-testid="authorizations-rule-family"
        >
          <option value="">{$t("dev.authorizations.family_any")}</option>
          {#each FAMILY_ORDER as family (family)}
            <option value={family}>{$t(FAMILY_LABEL[family])}</option>
          {/each}
        </Select>
      </div>

      <div class="space-y-1.5">
        <label class="text-caption text-muted-foreground" for="authorizations-rule-tool">
          {$t("dev.authorizations.field_tool")}
        </label>
        <Select id="authorizations-rule-tool" bind:value={draft.tool} data-testid="authorizations-rule-tool">
          <option value="" disabled>{$t("dev.authorizations.tool_placeholder")}</option>
          {#each offered as tool (tool.name)}
            <option value={tool.name}>{tool.name}</option>
          {/each}
          {#if draft.tool !== "" && !offered.some((tool) => tool.name === draft.tool)}
            <option value={draft.tool}>{draft.tool}</option>
          {/if}
        </Select>
      </div>

      <div class="space-y-1.5">
        <label class="text-caption text-muted-foreground" for="authorizations-rule-prefix">
          {$t("dev.authorizations.field_prefix")}
        </label>
        <Input
          id="authorizations-rule-prefix"
          bind:value={draft.argPrefix}
          placeholder={$t("dev.authorizations.prefix_placeholder")}
          data-testid="authorizations-rule-prefix"
        />
        <p class="m-0 text-caption text-muted-foreground">{$t("dev.authorizations.prefix_hint")}</p>
      </div>

      <div class="space-y-1.5">
        <span class="text-caption text-muted-foreground">{$t("dev.authorizations.field_scope")}</span>
        <div class="space-y-2" role="radiogroup" aria-label={$t("dev.authorizations.field_scope")}>
          <RadioItem
            value="all"
            checked={draft.scope === "all"}
            onchange={() => (draft.scope = "all")}
            data-testid="authorizations-rule-scope-all"
          >
            {$t("authorizations.scope_all_agents")}
          </RadioItem>
          <div class="flex flex-wrap items-center gap-3">
            <RadioItem
              value="agent"
              checked={draft.scope === "agent"}
              onchange={() => (draft.scope = "agent")}
              data-testid="authorizations-rule-scope-agent"
            >
              {$t("authorizations.scope_one_agent")}
            </RadioItem>
            <Select
              bind:value={draft.agentId}
              disabled={draft.scope !== "agent"}
              aria-label={$t("authorizations.scope_one_agent")}
              class="max-w-[16rem]"
              data-testid="authorizations-rule-agent"
            >
              {#each agents as agent (agent.id)}
                <option value={agent.id}>{agent.name}</option>
              {/each}
            </Select>
          </div>
        </div>
      </div>

      {#if touched && problem !== null}
        <p class="m-0 text-caption text-destructive" role="alert" data-testid="authorizations-rule-problem">
          {$t(DRAFT_PROBLEM_LABEL[problem])}
        </p>
      {/if}

      <DialogFooter>
        <Button type="button" variant="ghost" size="sm" onclick={onClose} disabled={saving} data-testid="authorizations-rule-cancel">
          {$t("common.cancel")}
        </Button>
        <Button type="submit" size="sm" loading={saving} disabled={saving} data-testid="authorizations-rule-save">
          {mode === "edit" ? $t("common.save") : $t("dev.authorizations.add_rule")}
        </Button>
      </DialogFooter>
    </form>
  </Dialog>
{/if}
