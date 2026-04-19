<script lang="ts">
  /**
   * Inline 1-click chat creation picker (US-SP42-024).
   *
   * Replaces the multi-step modal flow with an auto-focused textarea plus
   * clickable template and agent cards. Opens inline (no full-screen modal),
   * traps focus within its root, and persists expansion state to
   * `localStorage` so power users keep their preferred layout.
   */
  import { onMount, tick } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import {
    X,
    ChevronDown,
    ChevronRight,
    Sparkles,
    Bot,
    MessageSquare,
    BookOpen,
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
  import { projects } from "$lib/stores/projects";
  import { navigateTo } from "$lib/stores/navigation";
  import AgentStatusCard from "./AgentStatusCard.svelte";
  import TemplateCard from "./TemplateCard.svelte";

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
  let rootEl = $state<HTMLDivElement | null>(null);
  let textareaEl = $state<HTMLTextAreaElement | null>(null);
  let previouslyFocused: HTMLElement | null = null;

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
  onMount(() => {
    previouslyFocused = document.activeElement as HTMLElement | null;
    void tick().then(() => textareaEl?.focus());
    void invoke<AgentListItem[]>("list_agents")
      .then((list) => agents.set(list))
      .catch(() => { /* SSE will eventually populate */ });
    const stopPolling = startAgentStatusPolling();
    return () => {
      stopPolling();
      previouslyFocused?.focus();
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
        await invoke("send_chat_message", {
          sessionId: session.id,
          request: { content: initialPrompt.trim() },
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
        await invoke("send_chat_message", {
          sessionId: session.id,
          request: { content: prompt.trim() },
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
  function handleRootKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      onclose();
      return;
    }
    if (event.key === "Tab") {
      trapTab(event);
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

  /** Cycle Tab focus within the picker — required by B.58 focus trap. */
  function trapTab(event: KeyboardEvent): void {
    if (!rootEl) return;
    const focusables = Array.from(
      rootEl.querySelectorAll<HTMLElement>(
        'a[href], button:not([disabled]), textarea:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])',
      ),
    ).filter((el) => el.offsetParent !== null);

    if (focusables.length === 0) return;
    const first = focusables[0];
    const last = focusables[focusables.length - 1];
    const active = document.activeElement as HTMLElement | null;

    if (event.shiftKey && active === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && active === last) {
      event.preventDefault();
      first.focus();
    }
  }
</script>

<div
  bind:this={rootEl}
  class="glass-card glass-border animate-fade-in rounded-lg p-4"
  role="dialog"
  aria-modal="false"
  aria-label={$t("chat.quickpicker.title")}
  tabindex="-1"
  onkeydown={handleRootKeydown}
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
      bind:this={textareaEl}
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
        <!-- Empty state (B.59) -->
        <div
          class="flex flex-col items-center gap-2 rounded-lg border border-dashed border-border/50 bg-muted/20 px-4 py-6 text-center"
          data-testid="quickpicker-empty-agents"
        >
          <Bot size={18} class="text-muted-foreground/60" />
          <p class="text-[12px] text-muted-foreground">
            {$t("chat.quickpicker.empty_agents")}
          </p>
          <div class="flex gap-2">
            <Button size="sm" onclick={() => navigateTo("agents")}>
              {$t("chat.quickpicker.install_agent")}
            </Button>
            <a
              href="https://github.com/nidal-z/apollia-os/wiki/Agents"
              target="_blank"
              rel="noreferrer"
              class="inline-flex items-center gap-1 rounded-md px-2 py-1 text-[11px]
                text-muted-foreground hover:text-foreground
                focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40"
            >
              <BookOpen size={11} />
              {$t("chat.quickpicker.docs_link")}
            </a>
          </div>
        </div>
      {/if}
    {/if}
  </section>
</div>
