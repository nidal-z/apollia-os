<script lang="ts">
  /**
   * Inline 1-click chat creation picker.
   *
   * Replaces the multi-step modal flow with an auto-focused textarea plus
   * clickable template and agent cards. Opens inline (no full-screen modal),
   * traps focus within its root, and persists expansion state to
   * `localStorage` so power users keep their preferred layout.
   */
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import {
    X,
    ChevronDown,
    ChevronRight,
    Sparkles,
    MessageSquare,
  } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { LoadingSpinner } from "$lib/components/feedback";
  import type {
    AgentListItem,
    ChatSessionSummary,
    CreateSessionRequest,
  } from "$lib/types";
  import { agents } from "$lib/stores/sse";
  import {
    agentStatuses,
    startAgentStatusPolling,
    type AgentLiveStatus,
  } from "$lib/stores/agentStatus";
  import { CHAT_TEMPLATES, type ChatTemplate } from "$lib/templates/chatTemplates";
  import { triggerAutoName } from "$lib/chat/autoName";
  import { projects } from "$lib/stores/projects";
  import AgentStatusCard from "./AgentStatusCard.svelte";
  import EmptyAgentsState from "./EmptyAgentsState.svelte";
  import TemplateCard from "./TemplateCard.svelte";
  import { focusTrap } from "$lib/shortcuts/focusTrap";

  interface Props {
    /** Initial project binding for the created session. */
    defaultProjectId?: string | null;
    /** Called once the backend returns the created session. */
    oncreated: (session: ChatSessionSummary) => void;
    /** Called when the user dismisses the picker (Esc or close button). */
    onclose: () => void;
  }

  let { defaultProjectId = null, oncreated, onclose }: Props = $props();

  // ─── State ────────────────────────────────────────────────────────────────
  // Focus management is owned by `use:focusTrap` on the root — no DOM
  // refs needed beyond what the action grabs internally.

  let prompt = $state("");
  let creating = $state(false);
  let selectedProjectId = $state<string | null>(defaultProjectId);

  const EXPANDED_STORAGE_KEY = "apollia.quickpicker.expanded";
  interface ExpandedState {
    templates: boolean;
    agents: boolean;
  }
  const DEFAULT_EXPANDED: ExpandedState = { templates: true, agents: true };
  let expanded = $state<ExpandedState>(readExpanded());

  function readExpanded(): ExpandedState {
    if (typeof localStorage === "undefined") return DEFAULT_EXPANDED;
    try {
      const raw = localStorage.getItem(EXPANDED_STORAGE_KEY);
      if (!raw) return DEFAULT_EXPANDED;
      const parsed = JSON.parse(raw) as Partial<ExpandedState>;
      return { ...DEFAULT_EXPANDED, ...parsed };
    } catch {
      return DEFAULT_EXPANDED;
    }
  }

  function persistExpanded(next: ExpandedState): void {
    try {
      localStorage.setItem(EXPANDED_STORAGE_KEY, JSON.stringify(next));
    } catch {
      // Quota or private mode — degrade silently.
    }
  }

  function toggleSection(key: keyof ExpandedState): void {
    expanded = { ...expanded, [key]: !expanded[key] };
    persistExpanded(expanded);
  }

  // ─── Lifecycle ────────────────────────────────────────────────────────────
  // Focus management (initial autofocus + restore on close) is owned by
  // `use:focusTrap` on the root — this hook only seeds backend state.
  onMount(() => {
    void invoke<AgentListItem[]>("list_agents")
      .then((list) => agents.set(list))
      .catch(() => { /* SSE will eventually populate */ });
    const stopPolling = startAgentStatusPolling();
    return () => {
      stopPolling();
    };
  });

  // ─── Derived ──────────────────────────────────────────────────────────────
  const visibleAgents = $derived(
    $agents.filter((a) => a.agent_type !== "system"),
  );
  const hasAgents = $derived(visibleAgents.length > 0);

  function statusOf(name: string): AgentLiveStatus {
    return ($agentStatuses[name] ?? "offline") as AgentLiveStatus;
  }

  // ─── Actions ──────────────────────────────────────────────────────────────
  async function createFreeChat(initialPrompt?: string, tools?: string[]): Promise<void> {
    if (creating) return;
    creating = true;
    try {
      const request: CreateSessionRequest = {
        mode: "libre",
        project_id: selectedProjectId ?? undefined,
        tools,
      };
      const session = await invoke<ChatSessionSummary>("create_chat_session", {
        request,
      });
      if (initialPrompt && initialPrompt.trim().length > 0) {
        // Kick off auto-naming before send_chat_message so the title LLM call
        // runs concurrently with the agent run — title typically lands first.
        triggerAutoName(session.id, initialPrompt);
        await invoke("send_chat_message", {
          sessionId: session.id,
          content: initialPrompt.trim(),
        }).catch(() => { /* message can be retried manually */ });
      }
      oncreated(session);
    } finally {
      creating = false;
    }
  }

  async function createAgentChat(agentName: string): Promise<void> {
    if (creating) return;
    creating = true;
    try {
      const request: CreateSessionRequest = {
        mode: "agent",
        agent_name: agentName,
        project_id: selectedProjectId ?? undefined,
      };
      const session = await invoke<ChatSessionSummary>("create_chat_session", {
        request,
      });
      if (prompt.trim().length > 0) {
        triggerAutoName(session.id, prompt);
        await invoke("send_chat_message", {
          sessionId: session.id,
          content: prompt.trim(),
        }).catch(() => { /* user can retry */ });
      }
      oncreated(session);
    } finally {
      creating = false;
    }
  }

  function applyTemplate(template: ChatTemplate): void {
    prompt = template.prompt;
    void createFreeChat(template.prompt, template.tools);
  }

  // ─── Keyboard ─────────────────────────────────────────────────────────────
  // Tab cycling and focus restoration are handled by `use:focusTrap` on
  // the root — this handler only covers Escape (close) and the textarea
  // submit chord.
  function handleRootKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      onclose();
    }
  }

  function handleTextareaKeydown(event: KeyboardEvent): void {
    const isSubmit =
      (event.metaKey || event.ctrlKey) && event.key === "Enter";
    if (isSubmit) {
      event.preventDefault();
      if (prompt.trim().length === 0) return;
      void createFreeChat(prompt);
    }
  }
</script>

<div
  class="glass-card glass-border animate-fade-in rounded-lg p-4"
  role="dialog"
  aria-modal="false"
  aria-label={$t("chat.quickpicker.title")}
  tabindex="-1"
  onkeydown={handleRootKeydown}
  use:focusTrap={{ initialFocus: "textarea" }}
  data-testid="quickpicker"
>
  <!-- Header -->
  <div class="mb-3 flex items-center justify-between">
    <div class="flex items-center gap-2">
      <Sparkles size={13} class="text-primary/80" />
      <span class="text-xs font-medium">{$t("chat.quickpicker.title")}</span>
    </div>
    <button
      type="button"
      onclick={onclose}
      class="inline-flex h-6 w-6 items-center justify-center rounded text-muted-foreground/50 transition-colors hover:text-foreground
        focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40"
      aria-label={$t("a11y.close")}
      data-testid="quickpicker-close"
    >
      <X size={13} />
    </button>
  </div>

  <!-- Project selector -->
  {#if $projects.length > 0}
    <div class="mb-3 flex items-center gap-2">
      <span class="text-[11px] text-muted-foreground/70">
        {$t("chat.project_selector")}:
      </span>
      <select
        class="h-7 flex-1 rounded-md border border-border bg-card px-2 text-xs
          focus:outline-none focus:ring-1 focus:ring-primary/40"
        value={selectedProjectId ?? ""}
        onchange={(e) => {
          selectedProjectId = e.currentTarget.value || null;
        }}
        data-testid="quickpicker-project"
      >
        <option value="">{$t("chat.no_project")}</option>
        {#each $projects as proj (proj.id)}
          <option value={proj.id}>{proj.name}</option>
        {/each}
      </select>
    </div>
  {/if}

  <!-- Prompt textarea -->
  <label class="block">
    <span class="sr-only">{$t("chat.quickpicker.prompt_label")}</span>
    <textarea
      bind:value={prompt}
      onkeydown={handleTextareaKeydown}
      rows="3"
      placeholder={$t("chat.quickpicker.prompt_placeholder")}
      class="w-full resize-none rounded-md border border-border/60 bg-card/60 px-3 py-2 text-sm
        placeholder:text-muted-foreground/60
        focus:border-primary/40 focus:outline-none focus:ring-1 focus:ring-primary/30"
      data-testid="quickpicker-textarea"
    ></textarea>
  </label>

  <div class="mt-2 flex items-center justify-between">
    <span class="text-[10px] text-muted-foreground/50">
      {$t("chat.quickpicker.submit_hint")}
    </span>
    <Button
      size="sm"
      onclick={() => createFreeChat(prompt)}
      disabled={creating || prompt.trim().length === 0}
      data-testid="quickpicker-submit"
      class="gap-1.5"
    >
      {#if creating}
        <LoadingSpinner size={12} tone="current" />
      {:else}
        <MessageSquare size={12} />
      {/if}
      {$t("chat.quickpicker.start_free")}
    </Button>
  </div>

  <!-- Templates -->
  <section class="mt-4">
    <button
      type="button"
      class="mb-2 inline-flex items-center gap-1.5 text-[10px] font-semibold uppercase
        tracking-wider text-muted-foreground/60 transition-colors hover:text-foreground
        focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40 rounded"
      onclick={() => toggleSection("templates")}
      aria-expanded={expanded.templates}
      data-testid="quickpicker-toggle-templates"
    >
      {#if expanded.templates}
        <ChevronDown size={10} />
      {:else}
        <ChevronRight size={10} />
      {/if}
      {$t("chat.quickpicker.templates_section")}
    </button>
    {#if expanded.templates}
      <div class="grid grid-cols-1 gap-2 sm:grid-cols-2 lg:grid-cols-3" data-testid="quickpicker-templates">
        {#each CHAT_TEMPLATES as template (template.id)}
          <TemplateCard {template} disabled={creating} onselect={applyTemplate} />
        {/each}
      </div>
    {/if}
  </section>

  <!-- Agents -->
  <section class="mt-4">
    <button
      type="button"
      class="mb-2 inline-flex items-center gap-1.5 text-[10px] font-semibold uppercase
        tracking-wider text-muted-foreground/60 transition-colors hover:text-foreground
        focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40 rounded"
      onclick={() => toggleSection("agents")}
      aria-expanded={expanded.agents}
      data-testid="quickpicker-toggle-agents"
    >
      {#if expanded.agents}
        <ChevronDown size={10} />
      {:else}
        <ChevronRight size={10} />
      {/if}
      {$t("chat.quickpicker.agents_section")}
    </button>

    {#if expanded.agents}
      {#if hasAgents}
        <div class="grid grid-cols-1 gap-2 sm:grid-cols-2 lg:grid-cols-3" data-testid="quickpicker-agents">
          {#each visibleAgents as agent (agent.name)}
            <AgentStatusCard
              {agent}
              status={statusOf(agent.name)}
              disabled={creating}
              onselect={createAgentChat}
            />
          {/each}
        </div>
      {:else}
        <!-- Empty state — extracted to EmptyAgentsState for reuse. -->
        <EmptyAgentsState />
      {/if}
    {/if}
  </section>
</div>
