<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open as openFilePicker } from "@tauri-apps/plugin-dialog";
  import { t } from "svelte-i18n";
  import { Sheet } from "$lib/components/ui/sheet";
  import { LoadingSpinner } from "$lib/components/feedback";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Textarea } from "$lib/components/ui/textarea";
  import { addToast } from "$lib/components/ui/toast/store";
  import { navigateTo } from "$lib/stores/navigation";
  import { pendingChatSessionId } from "$lib/stores/chat";
  import {
    FolderOpen, FileText, Puzzle, Eye, Trash2, Upload, Plus,
    ChevronDown, ChevronUp, X, Loader2, MessageSquare, Bot,
    ToggleLeft, ToggleRight, GitBranch, FolderTree, ScrollText,
  } from "lucide-svelte";
  import type {
    ProjectDetail, WorkspaceSnapshotView, ChatSessionSummary, AgentListItem,
  } from "$lib/types";

  interface Props {
    open: boolean;
    project: ProjectDetail | null;
    loading: boolean;
    onclose: () => void;
    onupdated: () => void;
    ondelete: (id: string, name: string) => void;
  }

  let { open, project, loading, onclose, onupdated, ondelete }: Props = $props();

  // Edit state
  let editing = $state(false);
  let editName = $state("");
  let editDescription = $state("");
  let editInstructions = $state("");
  let saving = $state(false);

  // Snapshot preview
  let snapshotOpen = $state(false);
  let snapshot = $state<WorkspaceSnapshotView | null>(null);
  let snapshotLoading = $state(false);

  // Document upload
  let uploading = $state(false);

  // Linked chats
  let linkedChats = $state<ChatSessionSummary[]>([]);

  // Agent management
  let allAgents = $state<AgentListItem[]>([]);
  let showAgentPicker = $state(false);

  // Provider management
  let showProviderPicker = $state(false);

  const PROVIDER_TYPES = [
    { type: "git", name: "Git Status", priority: 10 },
    { type: "rules", name: "Project Rules (APOLLIA.md)", priority: 20 },
    { type: "tree", name: "Directory Tree", priority: 30 },
  ] as const;

  $effect(() => {
    if (project) {
      editName = project.name;
      editDescription = project.description ?? "";
      editInstructions = project.instructions ?? "";
      void loadLinkedChats(project.id);
      void loadAllAgents();
    }
    editing = false;
    snapshot = null;
    snapshotOpen = false;
  });

  async function loadLinkedChats(projectId: string): Promise<void> {
    try {
      linkedChats = await invoke<ChatSessionSummary[]>("list_chats_by_project", { projectId });
    } catch {
      linkedChats = [];
    }
  }

  async function loadAllAgents(): Promise<void> {
    try {
      allAgents = await invoke<AgentListItem[]>("list_agents");
    } catch {
      allAgents = [];
    }
  }

  async function createChatInProject(): Promise<void> {
    if (!project) return;
    try {
      const session = await invoke<ChatSessionSummary>("create_chat_session", {
        request: { mode: "libre", project_id: project.id },
      });
      pendingChatSessionId.set(session.id);
      navigateTo("chat");
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    }
  }

  function navigateToChatSession(sessionId: string): void {
    pendingChatSessionId.set(sessionId);
    navigateTo("chat");
  }

  async function addAgent(agentName: string): Promise<void> {
    if (!project) return;
    try {
      await invoke("add_project_agent", { projectId: project.id, agentName });
      addToast($t("projects.agent_added"), "success");
      showAgentPicker = false;
      onupdated();
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    }
  }

  async function removeAgent(agentName: string): Promise<void> {
    if (!project) return;
    try {
      await invoke("remove_project_agent", { projectId: project.id, agentName });
      addToast($t("projects.agent_removed"), "success");
      onupdated();
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    }
  }

  async function pickAndUploadDocument(): Promise<void> {
    if (!project) return;
    const selected = await openFilePicker({ multiple: false });
    if (!selected) return;
    uploading = true;
    try {
      const filePath = typeof selected === "string" ? selected : (selected as { path: string }).path;
      await invoke("upload_project_document", { projectId: project.id, filePath });
      onupdated();
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      uploading = false;
    }
  }

  async function addProvider(providerType: string, name: string, priority: number): Promise<void> {
    if (!project) return;
    try {
      await invoke("set_project_provider", {
        projectId: project.id,
        providerType,
        name,
        configJson: "{}",
        path: null,
        enabled: true,
        priority,
      });
      showProviderPicker = false;
      onupdated();
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    }
  }

  async function toggleProvider(providerId: string, enabled: boolean): Promise<void> {
    try {
      await invoke("toggle_project_provider", { providerId, enabled });
      onupdated();
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    }
  }

  async function removeProvider(providerId: string): Promise<void> {
    try {
      await invoke("remove_project_provider", { providerId });
      onupdated();
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    }
  }

  async function saveChanges(): Promise<void> {
    if (!project || !editName.trim()) return;
    saving = true;
    try {
      await invoke("update_project", {
        id: project.id,
        request: {
          name: editName.trim(),
          description: editDescription.trim() || null,
          instructions: editInstructions.trim() || null,
        },
      });
      addToast($t("projects.updated_toast", { values: { name: editName.trim() } }), "success");
      editing = false;
      onupdated();
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      saving = false;
    }
  }

  async function loadSnapshot(): Promise<void> {
    if (!project) return;
    snapshotLoading = true;
    try {
      snapshot = await invoke<WorkspaceSnapshotView>("get_project_snapshot", {
        projectId: project.id,
      });
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      snapshotLoading = false;
    }
  }

  function toggleSnapshot() {
    snapshotOpen = !snapshotOpen;
    if (snapshotOpen && !snapshot) {
      void loadSnapshot();
    }
  }

  async function removeDocument(docId: string): Promise<void> {
    try {
      await invoke("delete_project_document", { docId });
      addToast($t("projects.document_removed"), "success");
      onupdated();
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    }
  }
</script>

<Sheet {open} {onclose} class="w-full sm:max-w-[480px]">
  {#if loading || !project}
    <div class="flex flex-1 items-center justify-center py-16 text-muted-foreground text-sm">
      {#if loading}
        <LoadingSpinner size={20} tone="muted" class="mr-2" />
        {$t("common.loading")}
      {:else}
        {$t("projects.not_found")}
      {/if}
    </div>
  {:else}
    <!-- Header -->
    <div class="flex items-center gap-3 border-b border-border px-5 py-4">
      <FolderOpen size={20} strokeWidth={1.5} class="shrink-0 text-primary/70" />
      {#if editing}
        <Input
          bind:value={editName}
          class="flex-1 h-8 text-sm font-medium"
          autofocus
          aria-label={$t("projects.field_name")}
        />
      {:else}
        <h2 class="flex-1 text-base font-semibold truncate">{project.name}</h2>
      {/if}
      <button
        class="ml-auto h-7 w-7 inline-flex items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
        onclick={onclose}
        aria-label={$t("a11y.close")}
      >
        <X size={16} strokeWidth={1.75} />
      </button>
    </div>

    <!-- Body -->
    <div class="flex-1 overflow-y-auto px-5 py-4 space-y-5">

      <!-- Description -->
      <section class="space-y-1.5">
        <p class="text-xs font-semibold uppercase tracking-wider text-muted-foreground/60">
          {$t("projects.field_description")}
        </p>
        {#if editing}
          <Input
            bind:value={editDescription}
            placeholder={$t("projects.field_description_placeholder")}
            aria-label={$t("projects.field_description")}
          />
        {:else}
          <p class="text-sm text-muted-foreground">
            {project.description ?? $t("projects.no_description")}
          </p>
        {/if}
      </section>

      <!-- Workspace Path -->
      {#if project.workspace_path}
        <section class="space-y-1.5">
          <p class="text-xs font-semibold uppercase tracking-wider text-muted-foreground/60">
            {$t("projects.workspace_label")}
          </p>
          <p class="text-sm font-mono text-muted-foreground bg-muted/40 px-3 py-1.5 rounded-md truncate">
            {project.workspace_path}
          </p>
        </section>
      {/if}

      <!-- Instructions -->
      <section class="space-y-1.5">
        <p class="text-xs font-semibold uppercase tracking-wider text-muted-foreground/60">
          {$t("projects.field_instructions")}
        </p>
        {#if editing}
          <Textarea
            bind:value={editInstructions}
            rows={5}
            class="resize-none text-sm"
            placeholder={$t("projects.field_instructions_placeholder")}
            aria-label={$t("projects.field_instructions")}
          />
        {:else if project.instructions}
          <p class="text-sm whitespace-pre-wrap rounded-md bg-muted/50 px-3 py-2">
            {project.instructions}
          </p>
        {:else}
          <p class="text-sm text-muted-foreground">{$t("projects.no_instructions")}</p>
        {/if}
      </section>

      <!-- Linked Agents -->
      <section class="space-y-1.5">
        <div class="flex items-center justify-between">
          <p class="text-xs font-semibold uppercase tracking-wider text-muted-foreground/60">
            {$t("projects.section_agents")}
          </p>
          <Button size="sm" variant="ghost" onclick={() => { showAgentPicker = !showAgentPicker; }}>
            <Plus size={12} class="mr-1" />
            {$t("projects.add_agent")}
          </Button>
        </div>
        {#if showAgentPicker}
          <div class="rounded-md border border-border bg-muted/30 p-2 space-y-0.5">
            {#each allAgents.filter(a => a.enabled && !project.agents.includes(a.name)) as agent (agent.name)}
              <button
                class="flex w-full items-center gap-2.5 rounded-md px-2.5 py-1.5 text-sm hover:bg-muted/50 transition-colors text-left"
                onclick={() => addAgent(agent.name)}
              >
                <Bot size={14} strokeWidth={1.75} class="shrink-0 text-secondary" />
                <span class="truncate">{agent.name}</span>
              </button>
            {:else}
              <p class="text-xs text-muted-foreground px-2 py-1">{$t("projects.no_agents_available")}</p>
            {/each}
          </div>
        {/if}
        {#if project.agents.length === 0}
          <p class="text-sm text-muted-foreground">{$t("projects.no_agents")}</p>
        {:else}
          <div class="space-y-1">
            {#each project.agents as agentName (agentName)}
              <div class="flex items-center gap-2.5 rounded-md bg-muted/30 px-3 py-2 text-sm hover:bg-muted/50 transition-colors">
                <Bot size={14} strokeWidth={1.75} class="shrink-0 text-secondary" />
                <span class="flex-1 truncate">{agentName}</span>
                <button
                  class="h-5 w-5 inline-flex items-center justify-center rounded text-muted-foreground hover:text-destructive transition-colors"
                  onclick={() => removeAgent(agentName)}
                  title={$t("common.delete")}
                >
                  <Trash2 size={12} strokeWidth={1.75} />
                </button>
              </div>
            {/each}
          </div>
        {/if}
      </section>

      <!-- Context Providers -->
      <section class="space-y-1.5">
        <div class="flex items-center justify-between">
          <p class="text-xs font-semibold uppercase tracking-wider text-muted-foreground/60">
            {$t("projects.section_providers")}
          </p>
          <Button size="sm" variant="ghost" onclick={() => { showProviderPicker = !showProviderPicker; }}>
            <Plus size={12} class="mr-1" />
            {$t("projects.add_provider")}
          </Button>
        </div>
        {#if showProviderPicker}
          <div class="rounded-md border border-border bg-muted/30 p-2 space-y-0.5">
            {#each PROVIDER_TYPES.filter(pt => !project.providers.some(p => p.provider_type === pt.type)) as pt (pt.type)}
              <button
                class="flex w-full items-center gap-2.5 rounded-md px-2.5 py-1.5 text-sm hover:bg-muted/50 transition-colors text-left"
                onclick={() => addProvider(pt.type, pt.name, pt.priority)}
              >
                <Puzzle size={14} strokeWidth={1.75} class="shrink-0 text-primary" />
                <span class="flex-1">{pt.name}</span>
                <span class="text-[10px] text-muted-foreground/60 font-mono">{pt.type}</span>
              </button>
            {:else}
              <p class="text-xs text-muted-foreground px-2 py-1">{$t("projects.all_providers_added")}</p>
            {/each}
          </div>
        {/if}
        {#if project.providers.length === 0 && !showProviderPicker}
          <p class="text-sm text-muted-foreground">{$t("projects.no_providers")}</p>
        {:else if project.providers.length > 0}
          <div class="space-y-1">
            {#each project.providers as provider (provider.id)}
              {@const providerColor = provider.provider_type === "git" ? "text-secondary" : provider.provider_type === "tree" ? "text-primary" : "text-success"}
              <div class="flex items-center gap-2.5 rounded-md bg-muted/30 px-3 py-2 text-sm hover:bg-muted/50 transition-colors">
                {#if provider.provider_type === "git"}
                  <GitBranch size={14} strokeWidth={1.75} class="shrink-0 {providerColor}" />
                {:else if provider.provider_type === "tree"}
                  <FolderTree size={14} strokeWidth={1.75} class="shrink-0 {providerColor}" />
                {:else}
                  <ScrollText size={14} strokeWidth={1.75} class="shrink-0 {providerColor}" />
                {/if}
                <span class="flex-1 truncate">{provider.name}</span>
                <span class="text-[10px] text-muted-foreground/60 font-mono">{provider.provider_type}</span>
                <button
                  class="h-5 w-5 inline-flex items-center justify-center rounded text-muted-foreground hover:text-foreground transition-colors"
                  onclick={() => toggleProvider(provider.id, !provider.enabled)}
                  title={provider.enabled ? $t("projects.provider_disabled") : $t("projects.provider_enabled")}
                >
                  {#if provider.enabled}
                    <ToggleRight size={14} strokeWidth={1.75} class="text-success" />
                  {:else}
                    <ToggleLeft size={14} strokeWidth={1.75} />
                  {/if}
                </button>
                <button
                  class="h-5 w-5 inline-flex items-center justify-center rounded text-muted-foreground hover:text-destructive transition-colors"
                  onclick={() => removeProvider(provider.id)}
                  title={$t("common.delete")}
                >
                  <Trash2 size={12} strokeWidth={1.75} />
                </button>
              </div>
            {/each}
          </div>
        {/if}
      </section>

      <!-- Documents -->
      <section class="space-y-1.5">
        <div class="flex items-center justify-between">
          <p class="text-xs font-semibold uppercase tracking-wider text-muted-foreground/60">
            {$t("projects.section_documents")}
          </p>
          <Button size="sm" variant="ghost" onclick={pickAndUploadDocument} disabled={uploading}>
            {#if uploading}
              <Loader2 size={12} class="animate-spin mr-1" />
            {:else}
              <Upload size={12} class="mr-1" />
            {/if}
            {$t("projects.upload_document")}
          </Button>
        </div>
        {#if project.documents.length === 0}
          <p class="text-sm text-muted-foreground">{$t("projects.no_documents")}</p>
        {:else}
          <div class="space-y-1">
            {#each project.documents as doc (doc.id)}
              <div class="flex items-center gap-2.5 rounded-md bg-muted/30 px-3 py-2 text-sm hover:bg-muted/50 transition-colors">
                <FileText size={14} strokeWidth={1.75} class="shrink-0 text-primary" />
                <span class="flex-1 truncate">{doc.name}</span>
                <span class="text-xs text-muted-foreground/60">
                  {(doc.size_bytes / 1024).toFixed(1)} KB
                </span>
                <button
                  class="h-5 w-5 inline-flex items-center justify-center rounded text-muted-foreground hover:text-destructive transition-colors"
                  onclick={() => removeDocument(doc.id)}
                  title={$t("common.delete")}
                >
                  <Trash2 size={12} strokeWidth={1.75} />
                </button>
              </div>
            {/each}
          </div>
        {/if}
      </section>

      <!-- Linked Chats -->
      <section class="space-y-1.5">
        <div class="flex items-center justify-between">
          <p class="text-xs font-semibold uppercase tracking-wider text-muted-foreground/60">
            {$t("projects.section_chats")}
          </p>
          <Button size="sm" variant="ghost" onclick={createChatInProject}>
            <Plus size={12} class="mr-1" />
            {$t("projects.new_chat")}
          </Button>
        </div>
        {#if linkedChats.length === 0}
          <p class="text-sm text-muted-foreground">{$t("projects.no_chats")}</p>
        {:else}
          <div class="space-y-1">
            {#each linkedChats as chat (chat.id)}
              <button
                class="flex w-full items-center gap-2.5 rounded-md bg-muted/30 px-3 py-2 text-sm cursor-pointer hover:bg-muted/50 transition-colors text-left"
                onclick={() => navigateToChatSession(chat.id)}
              >
                <MessageSquare size={14} strokeWidth={1.75} class="shrink-0 text-primary" />
                <span class="flex-1 truncate">{chat.title || chat.mode}</span>
                <span class="text-xs text-muted-foreground/60">{chat.status}</span>
              </button>
            {/each}
          </div>
        {/if}
      </section>

      <!-- Workspace Snapshot Preview -->
      <section class="space-y-1.5">
        <button
          class="flex w-full items-center gap-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground/60 hover:text-foreground transition-colors"
          onclick={toggleSnapshot}
        >
          <Eye size={12} strokeWidth={2} />
          {$t("projects.section_snapshot")}
          {#if snapshotOpen}
            <ChevronUp size={12} class="ml-auto" />
          {:else}
            <ChevronDown size={12} class="ml-auto" />
          {/if}
        </button>
        {#if snapshotOpen}
          {#if snapshotLoading}
            <div class="flex items-center gap-2 py-4 justify-center text-sm text-muted-foreground">
              <Loader2 size={14} class="animate-spin" />
              {$t("common.loading")}
            </div>
          {:else if snapshot && snapshot.sections.length > 0}
            <div class="space-y-2">
              {#each snapshot.sections as section}
                {@const accentColor = section.source === "git" ? "text-secondary" : section.source === "tree" ? "text-primary" : "text-success"}
                <div class="rounded-md bg-muted/30 overflow-hidden">
                  <!-- Section header -->
                  <div class="flex items-center gap-2 px-3 py-1.5 border-b border-border/40">
                    {#if section.source === "git"}
                      <GitBranch size={12} strokeWidth={1.75} class={accentColor} />
                    {:else if section.source === "tree"}
                      <FolderTree size={12} strokeWidth={1.75} class={accentColor} />
                    {:else}
                      <ScrollText size={12} strokeWidth={1.75} class={accentColor} />
                    {/if}
                    <span class="text-xs font-medium text-foreground/70">{section.title}</span>
                    {#if section.source === "git"}
                      {@const lineCount = section.content.trim().split("\n").length}
                      <span class="ml-auto text-[10px] font-medium px-1.5 py-0.5 rounded-full bg-secondary/15 text-secondary">{lineCount}</span>
                    {/if}
                  </div>
                  <!-- Section content -->
                  <div class="max-h-36 overflow-y-auto">
                    {#if section.source === "git"}
                      {#each section.content.trim().split("\n") as line}
                        {@const status = line.trim().charAt(0)}
                        {@const filePath = line.trim().substring(2).trim()}
                        <div class="flex items-center gap-2 px-3 py-1 text-xs hover:bg-muted/40 transition-colors">
                          <span class="font-mono font-bold w-4 text-center shrink-0 {status === 'M' ? 'text-secondary' : status === 'A' ? 'text-success' : status === 'D' ? 'text-destructive' : 'text-muted-foreground'}">{status}</span>
                          <span class="font-mono text-foreground/60 truncate" title={filePath}>{filePath}</span>
                        </div>
                      {/each}
                    {:else if section.source === "tree"}
                      <div class="px-3 py-1.5">
                        {#each section.content.trim().split("\n") as line}
                          {@const isDir = line.trim().endsWith("/")}
                          <div class="flex items-center gap-1.5 py-0.5 text-xs font-mono {isDir ? 'text-primary font-medium' : 'text-foreground/50'}">
                            {#if isDir}
                              <FolderOpen size={10} strokeWidth={2} class="shrink-0 text-primary" />
                            {:else}
                              <FileText size={10} strokeWidth={1.5} class="shrink-0 text-muted-foreground/40" />
                            {/if}
                            <span class="truncate">{line.trim()}</span>
                          </div>
                        {/each}
                      </div>
                    {:else}
                      <pre class="text-xs px-3 py-2 whitespace-pre-wrap text-foreground/60 font-mono leading-relaxed">{section.content}</pre>
                    {/if}
                  </div>
                </div>
              {/each}
            </div>
          {:else}
            <p class="text-sm text-muted-foreground">{$t("projects.snapshot_empty")}</p>
          {/if}
        {/if}
      </section>
    </div>

    <!-- Footer actions -->
    <div class="border-t border-border px-5 py-3 flex items-center gap-2">
      {#if editing}
        <Button size="sm" onclick={saveChanges} disabled={saving || !editName.trim()}>
          {saving ? $t("common.submitting") : $t("common.save")}
        </Button>
        <Button size="sm" variant="outline" onclick={() => { editing = false; }} disabled={saving}>
          {$t("common.cancel")}
        </Button>
      {:else}
        <Button size="sm" variant="outline" onclick={() => { editing = true; }}>
          {$t("projects.edit_project")}
        </Button>
      {/if}
      <div class="flex-1"></div>
      <Button
        size="sm"
        variant="ghost"
        class="text-destructive hover:bg-destructive/10"
        onclick={() => ondelete(project.id, project.name)}
      >
        <Trash2 size={14} strokeWidth={1.75} class="mr-1.5" />
        {$t("common.delete")}
      </Button>
    </div>
  {/if}
</Sheet>
