<script lang="ts">
  /**
   * AgentDetailHeader - the detail-pane header for a selected assistant.
   *
   * Wraps the canonical `DetailHeader`: a signature-gradient icon chip (the
   * single expressive focal point), the name, the description, the "New chat"
   * primary action, the update and uninstall actions, and a footer row of
   * status / version / tools / class / A2A badges.
   *
   * The uninstall used to live on `AgentCard`, deleted as an unused component
   * in April. `uninstall_agent` stayed registered and lost every caller, so an
   * installed agent became impossible to remove from the interface. The confirm
   * mirrors `PackageDetail`, with the memory option the card carried.
   *
   * `update_agent` had the same fate: registered, never called, so replacing an
   * agent's file meant uninstall then reinstall, which drops the auto-start
   * flag. The update picks a new `.py` the same way the install does; the
   * failure it reports goes to the route banner through `agentActions`.
   *
   * Replacing the file of a running agent stops it and starts it again, since
   * the interpreter would otherwise keep serving the module it imported at
   * start time. That is an interruption the operator has to agree to, so the
   * picked file goes through a confirm step first, the same shape as the
   * uninstall one. An agent that is not running skips it: nothing to interrupt.
   */
  import { t } from "svelte-i18n";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { MessageSquare, RefreshCw, Sparkles, Trash2 } from "lucide-svelte";
  import { reportError } from "$lib/errors/reportError";
  import { DetailHeader, StatusDot } from "$lib/components/operator";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import type { AgentActions } from "./useAgentActions.svelte";
  import type { AgentListItem } from "$lib/types";
  import {
    agentClassLabel,
    isActive,
    statusColor,
    statusLabel,
    statusTone,
  } from "./agentStatus";

  interface Props {
    agent: AgentListItem;
    agentActions: AgentActions;
    /**
     * Whether the uninstall confirm is armed. Owned by the route rather than
     * here, because the sidebar's context menu arms it for an agent that is
     * not the selected one yet: it selects and arms in the same gesture.
     */
    confirmingUninstall: boolean;
    onArmUninstall: () => void;
    onDisarmUninstall: () => void;
    onStartChat: (name: string) => void;
  }

  let {
    agent,
    agentActions,
    confirmingUninstall,
    onArmUninstall,
    onDisarmUninstall,
    onStartChat,
  }: Props = $props();

  const toolCount = $derived(
    agent.tools_required.length + agent.tools_optional.length,
  );

  let deleteMemory = $state(false);
  let updating = $state(false);
  /**
   * File picked for the replacement, held while the operator decides.
   *
   * Non-null only between the picker closing and the confirm, and only for a
   * running agent. Holding the path rather than reopening the picker after the
   * confirm keeps the operator from choosing the file twice.
   */
  let pendingUpdatePath = $state<string | null>(null);
  const busy = $derived(agentActions.busyKeys[`agent:${agent.name}`] === true);
  const installed = $derived(agent.installed_at !== null);
  // Same set of live states the command checks before cycling the agent, so
  // the warning appears exactly when a restart will happen.
  const runningNow = $derived(
    isActive(agent) || agent.runtime_status === "initializing",
  );

  // Neither the memory option nor a pending replacement survives a change of
  // agent: both are decisions about one agent, and carrying them across would
  // act on something the operator never chose them for.
  $effect(() => {
    void agent.name;
    deleteMemory = false;
    pendingUpdatePath = null;
  });

  // Arming the uninstall retires a pending replacement: both confirms claim
  // the same corner of the header, and the operator answered only one of them.
  $effect(() => {
    if (confirmingUninstall) pendingUpdatePath = null;
  });

  /**
   * Pick a replacement `.py`, then either confirm the restart or apply it.
   *
   * Same native filter as the install picker, so the two entry points into the
   * loader agree on what a candidate file looks like.
   */
  async function pickAndUpdate(): Promise<void> {
    agentActions.clearUpdateError();
    let path: string | null = null;
    try {
      path = await openDialog({
        filters: [{ name: "Python Agent", extensions: ["py"] }],
        multiple: false,
      });
    } catch (err) {
      reportError(err, { surface: "toast" });
      return;
    }
    if (!path) return;
    if (runningNow) {
      pendingUpdatePath = path;
      return;
    }
    await applyUpdate(path);
  }

  async function applyUpdate(path: string): Promise<void> {
    updating = true;
    try {
      await agentActions.update(agent, path);
    } finally {
      updating = false;
      pendingUpdatePath = null;
    }
  }

  function confirmUpdate(): void {
    const path = pendingUpdatePath;
    if (path === null) return;
    void applyUpdate(path);
  }

  function confirm(): void {
    const wanted = deleteMemory;
    onDisarmUninstall();
    void agentActions.uninstall(agent, wanted);
  }
</script>

<DetailHeader
  title={agent.name}
  titleTestid="agent-detail-title"
  meta={agent.description ?? $t("agents.no_description")}
>
  {#snippet leading()}
    <span
      class="inline-flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-gradient-primary text-primary-foreground shadow-elev-2"
    >
      <Sparkles size={18} />
    </span>
  {/snippet}
  {#snippet actions()}
    {#if confirmingUninstall}
      <div class="flex flex-col items-end gap-1.5">
        <span class="text-body-xs font-medium text-destructive">
          {$t("agents.uninstall_confirm_warning", { values: { name: agent.name } })}
        </span>
        <label class="flex cursor-pointer select-none items-center gap-1.5">
          <Checkbox bind:checked={deleteMemory} data-testid="agent-uninstall-memory" />
          <span class="text-caption text-muted-foreground">
            {$t("agents.uninstall_delete_memory")}
          </span>
        </label>
        <div class="flex items-center gap-2">
          <Button variant="outline" size="sm" onclick={onDisarmUninstall}>
            {$t("common.cancel")}
          </Button>
          <Button
            variant="destructive"
            size="sm"
            disabled={busy}
            onclick={confirm}
            data-testid="agent-uninstall-confirm"
          >
            {#snippet icon()}<Trash2 size={12} />{/snippet}
            {$t("agents.uninstall_confirm_action")}
          </Button>
        </div>
      </div>
    {:else if pendingUpdatePath !== null}
      <div class="flex flex-col items-end gap-1.5" data-testid="agent-update-confirm-panel">
        <span class="text-body-xs font-medium text-warning-a11y">
          {$t("agents.update_restart_warning", { values: { name: agent.name } })}
        </span>
        <div class="flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            disabled={updating}
            onclick={() => (pendingUpdatePath = null)}
            data-testid="agent-update-cancel"
          >
            {$t("common.cancel")}
          </Button>
          <Button
            variant="primary-solid"
            size="sm"
            disabled={busy || updating}
            onclick={confirmUpdate}
            data-testid="agent-update-confirm"
          >
            {#snippet icon()}<RefreshCw size={12} />{/snippet}
            {updating ? $t("agents.updating") : $t("agents.update_restart_confirm")}
          </Button>
        </div>
      </div>
    {:else}
      {#if installed}
        <Button
          variant="outline"
          size="sm"
          disabled={busy || updating}
          title={$t("agents.update_hint")}
          onclick={pickAndUpdate}
          data-testid="agent-update-btn"
        >
          {#snippet icon()}<RefreshCw size={12} />{/snippet}
          {updating ? $t("agents.updating") : $t("agents.update")}
        </Button>
        <Button
          variant="outline"
          size="sm"
          disabled={updating}
          onclick={onArmUninstall}
          data-testid="agent-uninstall-btn"
        >
          {#snippet icon()}<Trash2 size={12} />{/snippet}
          {$t("agents.uninstall")}
        </Button>
      {/if}
      <Button
        variant="primary-solid"
        size="sm"
        onclick={() => onStartChat(agent.name)}
        disabled={!isActive(agent)}
      >
        {#snippet icon()}<MessageSquare size={12} />{/snippet}
        {$t("agents.new_chat")}
      </Button>
    {/if}
  {/snippet}
  {#snippet footer()}
    <Badge size="sm" variant={statusTone(agent)}>
      {#snippet icon()}
        <StatusDot
          color={statusColor(agent)}
          glow={agent.runtime_status === "active"}
          size={5}
        />
      {/snippet}
      {statusLabel(agent, $t)}
    </Badge>
    <Badge size="sm" variant="neutral">v{agent.version}</Badge>
    <Badge size="sm" variant="neutral">
      {toolCount} {$t("agents.tools_word")}
    </Badge>
    {#if agentClassLabel(agent)}
      <Badge size="sm" variant="info">{agentClassLabel(agent)}</Badge>
    {/if}
    {#if agent.execution_mode}
      <Badge size="sm" variant="neutral">{agent.execution_mode}</Badge>
    {/if}
    {#if agent.supports_a2a}
      <Badge size="sm" variant="info">A2A</Badge>
    {/if}
  {/snippet}
</DetailHeader>
