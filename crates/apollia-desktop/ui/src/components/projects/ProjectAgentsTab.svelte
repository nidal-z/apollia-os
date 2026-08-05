<script lang="ts">
  /**
   * ProjectAgentsTab - agents attached to the project.
   *
   * Owns the three link mutations (`listProjectAgents`, `addProjectAgent`,
   * `removeProjectAgent`) and keeps its own copy of the attached names so a
   * mutation repaints the list immediately. `onChanged` lets the parent reload
   * the project detail, which is what refreshes the header count and the tasks
   * narrowed by agent. Candidates come from the installed-agents store.
   *
   * A detach is destructive enough to deserve the two-step inline confirm the
   * rest of the application uses (`AgentDetailHeader`): the first click arms
   * the row, the second one commits.
   */
  import { t } from "svelte-i18n";
  import { Sparkles, Plus, X } from "lucide-svelte";
  import {
    SectionTitle,
    EmptyState,
    ErrorBanner,
    SkeletonList,
    StatusDot,
  } from "$lib/components/operator";
  import { Button } from "$lib/components/ui/button";
  import { Select } from "$lib/components/ui/select";
  import { addToast } from "$lib/components/ui/toast/store";
  import { reportError } from "$lib/errors/reportError";
  import type { HumanizedError } from "$lib/errors/humanize";
  import { installedAgents } from "$lib/stores/agents";
  import {
    listProjectAgents,
    addProjectAgent,
    removeProjectAgent,
  } from "$lib/ipc/projects";
  import type { AgentListItem, ProjectDetail } from "$lib/types";

  interface Props {
    project: ProjectDetail;
    /** Called after an attach or a detach so the parent can reload the detail. */
    onChanged: () => void | Promise<void>;
  }

  let { project, onChanged }: Props = $props();

  // Seeded from the detail already loaded by the parent, then kept in sync with
  // the dedicated read so the tab never shows a stale link set.
  // svelte-ignore state_referenced_locally
  let attached = $state<string[]>([...project.agents]);
  let loading = $state(false);
  let agentError = $state<HumanizedError | null>(null);
  let pendingName = $state<string | null>(null);
  let selectedAgent = $state("");
  // Name of the row whose detach confirm is armed, null when none is.
  let confirmingName = $state<string | null>(null);

  void refreshAttached();

  const attachedSet = $derived(new Set(attached));
  const candidates = $derived(
    $installedAgents.filter((agent) => !attachedSet.has(agent.name)),
  );

  // The picked agent can disappear from the candidates (attached elsewhere,
  // uninstalled); drop the stale selection instead of submitting it.
  $effect(() => {
    if (selectedAgent && !candidates.some((a) => a.name === selectedAgent)) {
      selectedAgent = "";
    }
  });

  // An armed row that leaves the list (detached elsewhere, refresh) must not
  // stay armed for whatever name lands at that position next.
  $effect(() => {
    if (confirmingName && !attachedSet.has(confirmingName)) {
      confirmingName = null;
    }
  });

  function describe(name: string): AgentListItem | undefined {
    return $installedAgents.find((agent) => agent.name === name);
  }

  async function refreshAttached(): Promise<void> {
    loading = true;
    try {
      attached = await listProjectAgents(project.id);
      agentError = null;
    } catch (err: unknown) {
      agentError = reportError(err, { surface: "inline" });
    } finally {
      loading = false;
    }
  }

  async function attachAgent(): Promise<void> {
    const name = selectedAgent;
    if (!name) return;
    agentError = null;
    pendingName = name;
    try {
      await addProjectAgent(project.id, name);
      addToast($t("projects.agent_added"), "success");
      selectedAgent = "";
      await refreshAttached();
      await onChanged();
    } catch (err: unknown) {
      agentError = reportError(err, { surface: "inline" });
    } finally {
      pendingName = null;
    }
  }

  async function detachAgent(name: string): Promise<void> {
    agentError = null;
    confirmingName = null;
    pendingName = name;
    try {
      await removeProjectAgent(project.id, name);
      addToast($t("projects.agent_removed"), "success");
      await refreshAttached();
      await onChanged();
    } catch (err: unknown) {
      agentError = reportError(err, { surface: "inline" });
    } finally {
      pendingName = null;
    }
  }
</script>

<SectionTitle count={attached.length}>
  {$t("projects.section_agents")}
</SectionTitle>

<div class="px-8 pb-6 space-y-3">
  {#if agentError}
    <ErrorBanner data-testid="project-agent-error">
      <p class="font-medium">{agentError.friendly_message}</p>
      <p class="text-caption opacity-90">{agentError.suggested_action}</p>
      {#if agentError.detail}
        <details class="mt-2">
          <summary
            class="cursor-pointer text-caption opacity-70 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
            data-testid="project-agent-error-details"
          >
            {$t("errors.show_details")}
          </summary>
          <p class="mt-1.5 break-all font-mono text-code-sm opacity-80">
            {agentError.detail}
          </p>
        </details>
      {/if}
    </ErrorBanner>
  {/if}

  <p class="text-caption text-muted-foreground">{$t("projects.agents_hint")}</p>

  <div class="flex items-center gap-2 max-w-xl">
    <Select
      bind:value={selectedAgent}
      size="sm"
      class="flex-1 bg-card"
      disabled={candidates.length === 0 || pendingName !== null}
      aria-label={$t("projects.add_agent")}
      data-testid="project-agent-select"
    >
      <option value="">
        {candidates.length === 0
          ? $t("projects.agents_all_attached")
          : $t("projects.agent_pick_placeholder")}
      </option>
      {#each candidates as agent (agent.name)}
        <option value={agent.name}>{agent.name}</option>
      {/each}
    </Select>
    <Button
      variant="primary-solid"
      size="sm"
      type="button"
      onclick={attachAgent}
      disabled={!selectedAgent || pendingName !== null}
      data-testid="project-agent-add-btn"
    >
      {#snippet icon()}<Plus size={12} />{/snippet}
      {$t("projects.add_agent")}
    </Button>
  </div>

  {#if loading && attached.length === 0}
    <SkeletonList count={2} avatar={false} rowClass="px-4 py-2" />
  {:else if attached.length === 0}
    <EmptyState
      title={$t("projects.no_agents")}
      desc={$t("projects.no_agents_desc")}
    >
      {#snippet icon()}<Sparkles size={20} />{/snippet}
    </EmptyState>
  {:else}
    <ul class="divide-y divide-border border border-border rounded-xl bg-card overflow-hidden">
      {#each attached as name (name)}
        {@const agent = describe(name)}
        <li class="px-4 py-2.5 flex items-center gap-2.5" data-testid="project-agent-row-{name}">
          <Sparkles size={13} class="text-muted-foreground" />
          <span class="text-body-xs text-foreground truncate">{name}</span>
          {#if agent?.runtime_status === "active"}
            <StatusDot color="hsl(var(--success))" glow />
          {/if}
          {#if confirmingName === name}
            <span class="text-caption font-medium text-destructive truncate flex-1">
              {$t("projects.detach_agent_confirm", { values: { name } })}
            </span>
            <Button
              variant="outline"
              size="sm"
              type="button"
              onclick={() => (confirmingName = null)}
              data-testid="project-agent-remove-cancel-{name}"
            >
              {$t("common.cancel")}
            </Button>
            <Button
              variant="destructive"
              size="sm"
              type="button"
              onclick={() => detachAgent(name)}
              disabled={pendingName === name}
              data-testid="project-agent-remove-confirm-{name}"
            >
              {#snippet icon()}<X size={12} />{/snippet}
              {$t("projects.detach_agent_confirm_action")}
            </Button>
          {:else}
            <span class="text-caption text-muted-foreground truncate flex-1">
              {agent?.description ?? $t("projects.agent_not_installed")}
            </span>
            <Button
              variant="ghost"
              size="icon-sm"
              type="button"
              onclick={() => (confirmingName = name)}
              disabled={pendingName === name}
              aria-label={$t("projects.remove_agent")}
              title={$t("projects.remove_agent")}
              data-testid="project-agent-remove-{name}"
            >
              <X size={12} />
            </Button>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</div>
