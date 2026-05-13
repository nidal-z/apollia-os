<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { slide } from "svelte/transition";
  import { ShieldAlert, Zap, ChevronDown } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { currentSession } from "$lib/stores/chat";
  import RejectReasonDialog from "../inbox/RejectReasonDialog.svelte";

  interface Props {
    sessionId: string;
    messageId: string;
    toolName: string;
    inputPreview: string;
  }

  let { sessionId, messageId, toolName, inputPreview }: Props = $props();

  /** True quand la session courante est rattachée à un projet — sert à griser
   *  l'option "Toujours pour ce projet" hors contexte projet. */
  const hasProject = $derived(
    $currentSession?.id === sessionId && $currentSession?.project_id !== null,
  );

  /**
   * Portée de la règle "Toujours autoriser".
   * Mappe sur l'enum runtime `AlwaysAcceptScope` (snake_case via serde).
   */
  type AlwaysScope = "this_session" | "this_agent" | "this_project" | "global";

  let isProcessing = $state(false);
  let error = $state<string | null>(null);
  let scopeOpen = $state(false);
  let rejectDialogOpen = $state(false);

  /** True when this is an A2A worker-agent delegation. */
  const isA2A = $derived(toolName.startsWith("a2a:"));
  /** Skill ID (e.g. "read-excel") extracted from "a2a:read-excel". */
  const a2aSkillId = $derived(isA2A ? toolName.slice(4) : null);

  async function handleAccept(): Promise<void> {
    isProcessing = true;
    error = null;
    try {
      await invoke("authorize_chat_tool", {
        sessionId,
        messageId,
        toolName,
        decision: "accept",
      });
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
      isProcessing = false;
    }
  }

  /** Refusal goes through a mandatory reason dialog (5-500 chars).
   *  The reason is forwarded to the agent loop so the LLM can correct course. */
  async function handleRefuse(reason: string): Promise<void> {
    isProcessing = true;
    error = null;
    try {
      await invoke("authorize_chat_tool", {
        sessionId,
        messageId,
        toolName,
        decision: "refuse",
        reason,
      });
      rejectDialogOpen = false;
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
      isProcessing = false;
    }
  }

  async function handleAlwaysAccept(scope: AlwaysScope): Promise<void> {
    isProcessing = true;
    error = null;
    try {
      await invoke("authorize_chat_tool", {
        sessionId,
        messageId,
        toolName,
        decision: "always_accept",
        scope,
      });
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
      isProcessing = false;
      scopeOpen = false;
    }
  }
</script>

<div class="glass-card glass-border my-1.5 rounded-lg border-l-2 px-3 py-2.5 text-xs {isA2A ? 'border-l-secondary' : 'border-l-warning'}" data-testid="approval-card-{toolName}" transition:slide={{ duration: 200 }}>
  <div class="flex items-center gap-2 font-medium text-foreground">
    <div
      class="flex h-6 w-6 items-center justify-center rounded-lg {isA2A
        ? 'bg-secondary/10'
        : 'bg-warning/10'}"
    >
      {#if isA2A}
        <Zap class="h-3.5 w-3.5 text-secondary" />
      {:else}
        <ShieldAlert class="h-3.5 w-3.5 text-warning" />
      {/if}
    </div>
    {#if isA2A}
      <span>
        {$t("chat.a2a_authorize_skill")}
        <strong>{a2aSkillId}</strong>
        {$t("chat.a2a_via_worker")} ?
      </span>
    {:else}
      <span>{$t("chat.authorize_tool")} <strong>{toolName}</strong> ?</span>
    {/if}
  </div>

  <pre class="mt-2 whitespace-pre-wrap break-all rounded-lg glass-surface p-2 font-mono text-[10px] text-muted-foreground">{inputPreview}</pre>

  {#if error}
    <p class="mt-1.5 text-[10px] text-destructive">{error}</p>
  {/if}

  <div class="mt-2.5 flex flex-wrap items-center gap-2">
    <Button
      variant="outline"
      size="sm"
      class="h-7 px-3 text-[11px]"
      disabled={isProcessing}
      onclick={handleAccept}
      data-testid="approval-accept-{toolName}"
    >
      Autoriser une fois
    </Button>
    <Button
      variant="ghost"
      size="sm"
      class="h-7 px-3 text-[11px] text-destructive hover:bg-destructive/10 hover:text-destructive"
      disabled={isProcessing}
      onclick={() => (rejectDialogOpen = true)}
      data-testid="approval-refuse-{toolName}"
    >
      Refuser
    </Button>
    <Button
      variant="ghost"
      size="sm"
      class="h-7 px-3 text-[11px] text-primary"
      disabled={isProcessing}
      onclick={() => (scopeOpen = !scopeOpen)}
      data-testid="approval-always-toggle-{toolName}"
      aria-expanded={scopeOpen}
    >
      Toujours autoriser
      <ChevronDown
        size={11}
        class="ml-0.5 transition-transform {scopeOpen ? 'rotate-180' : ''}"
      />
    </Button>
  </div>

  {#if scopeOpen}
    <div
      class="mt-2 grid grid-cols-1 gap-1.5 rounded-md bg-surface-2/40 p-2 sm:grid-cols-2"
      transition:slide={{ duration: 150 }}
      data-testid="approval-scope-menu-{toolName}"
    >
      <Button variant="ghost" size="sm"
        type="button"
        class="rounded-md px-2.5 py-1.5 text-left text-[11px] hover:bg-surface-1 disabled:opacity-50"
        disabled={isProcessing}
        onclick={() => handleAlwaysAccept("this_session")}
        data-testid="approval-scope-session-{toolName}"
      >
        <div class="font-medium text-foreground">Pour cette session</div>
        <div class="text-[10px] text-muted-foreground">
          Jusqu'à la fermeture de ce chat.
        </div>
      </Button>
      <Button variant="ghost" size="sm"
        type="button"
        class="rounded-md px-2.5 py-1.5 text-left text-[11px] hover:bg-surface-1 disabled:opacity-50"
        disabled={isProcessing}
        onclick={() => handleAlwaysAccept("this_agent")}
        data-testid="approval-scope-agent-{toolName}"
      >
        <div class="font-medium text-foreground">Toujours pour cet assistant</div>
        <div class="text-[10px] text-muted-foreground">
          Apollia Chat (ou l'agent courant) ne demandera plus.
        </div>
      </Button>
      <Button variant="ghost" size="sm"
        type="button"
        class="rounded-md px-2.5 py-1.5 text-left text-[11px] hover:bg-surface-1 disabled:cursor-not-allowed disabled:opacity-50 disabled:hover:bg-transparent"
        disabled={isProcessing || !hasProject}
        onclick={() => handleAlwaysAccept("this_project")}
        data-testid="approval-scope-project-{toolName}"
        title={!hasProject ? "Cette session n'est rattachée à aucun projet." : undefined}
      >
        <div class="font-medium text-foreground">Toujours pour ce projet</div>
        <div class="text-[10px] text-muted-foreground">
          {hasProject
            ? "Tous les assistants utilisés dans ce projet."
            : "Indisponible — la session n'est rattachée à aucun projet."}
        </div>
      </Button>
      <Button variant="ghost" size="sm"
        type="button"
        class="rounded-md px-2.5 py-1.5 text-left text-[11px] hover:bg-surface-1 disabled:opacity-50"
        disabled={isProcessing}
        onclick={() => handleAlwaysAccept("global")}
        data-testid="approval-scope-global-{toolName}"
      >
        <div class="font-medium text-foreground">Toujours, partout</div>
        <div class="text-[10px] text-warning">
          Tous les assistants, tous les projets.
        </div>
      </Button>
    </div>
  {/if}
</div>

<RejectReasonDialog
  open={rejectDialogOpen}
  submitting={isProcessing}
  onclose={() => (rejectDialogOpen = false)}
  onconfirm={handleRefuse}
/>
