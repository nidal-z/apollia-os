<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { Terminal, FileText, Code, Save, Cpu, Settings2 } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Toggle } from "$lib/components/ui/toggle";
  import { Sheet } from "$lib/components/ui/sheet";
  import type { ChatSessionDetail, UpdateSessionRequest } from "$lib/types";
  import { llmBackends } from "$lib/stores/sse";

  interface Props {
    open: boolean;
    session: ChatSessionDetail | null;
    onclose: () => void;
    onupdated: () => void;
  }

  let { open, session, onclose, onupdated }: Props = $props();

  let systemPrompt = $state("");
  let toolBash = $state(false);
  let toolFileIo = $state(false);
  let toolPython = $state(false);
  let selectedBackend = $state<string | null>(null);
  let saving = $state(false);
  let saveError = $state<string | null>(null);

  const isClosed = $derived(session?.status === "closed");
  const isProcessing = $derived(session?.status === "processing");
  const isReadOnly = $derived(isClosed || isProcessing);

  const TOOL_META: { id: string; icon: typeof Terminal; label: string }[] = [
    { id: "bash_executor", icon: Terminal, label: "bash_executor" },
    { id: "file_io", icon: FileText, label: "file_io" },
    { id: "python_executor", icon: Code, label: "python_executor" },
  ];

  const selectedTools = $derived.by(() => {
    const tools: string[] = [];
    if (toolBash) tools.push("bash_executor");
    if (toolFileIo) tools.push("file_io");
    if (toolPython) tools.push("python_executor");
    return tools;
  });

  function syncFromSession(): void {
    if (!session) return;
    systemPrompt = session.system_prompt ?? "";
    toolBash = (session.available_tools ?? []).includes("bash_executor");
    toolFileIo = (session.available_tools ?? []).includes("file_io");
    toolPython = (session.available_tools ?? []).includes("python_executor");
    selectedBackend = session.llm_backend ?? null;
    saveError = null;
  }

  async function handleSave(): Promise<void> {
    if (!session || isReadOnly) return;
    saving = true;
    saveError = null;
    try {
      const update: UpdateSessionRequest = {
        system_prompt: systemPrompt.trim(),
        tools: selectedTools,
        llm_backend: selectedBackend,
      };
      await invoke("update_chat_session", { sessionId: session.id, update });
      onupdated();
      onclose();
    } catch (err: unknown) {
      saveError = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }

  function getToolChecked(toolId: string): boolean {
    if (toolId === "bash_executor") return toolBash;
    if (toolId === "file_io") return toolFileIo;
    return toolPython;
  }

  function setToolChecked(toolId: string, checked: boolean): void {
    if (toolId === "bash_executor") toolBash = checked;
    else if (toolId === "file_io") toolFileIo = checked;
    else toolPython = checked;
  }

  $effect(() => {
    if (open) syncFromSession();
  });
</script>

<Sheet {open} onclose={onclose}>
  <div class="flex h-full flex-col" data-testid="chat-config-panel">
    <!-- Header card -->
    <div class="px-5 py-4 border-b glass-border-subtle">
      <div class="flex items-center gap-2.5">
        <div class="flex h-8 w-8 items-center justify-center rounded-lg bg-primary/10">
          <Settings2 class="h-4 w-4 text-primary" />
        </div>
        <h3 class="text-[13px] font-medium">{$t("chat.config_title")}</h3>
      </div>
    </div>

    <!-- Body -->
    <div class="flex-1 overflow-y-auto px-5 py-5 space-y-4">
      <!-- LLM Provider section -->
      <div class="glass-card glass-border rounded-xl p-3.5 space-y-2.5">
        <p class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/50">
          {$t("chat.config_provider")}
        </p>
        {#if $llmBackends.length > 0}
          <div class="space-y-1.5">
            {#each $llmBackends as backend (backend.name)}
              <button
                class="w-full flex items-center gap-3 rounded-lg px-3 py-2.5 text-left transition-all
                  {(selectedBackend === backend.name || (selectedBackend === null && backend.name === $llmBackends[0]?.name))
                    ? 'glass-card ring-2 ring-primary/40 text-foreground'
                    : 'glass-inset text-muted-foreground hover:glass-surface hover:text-foreground'}"
                disabled={isReadOnly}
                onclick={() => (selectedBackend = backend.name)}
                data-testid="config-backend-{backend.name}"
              >
                <div class="flex h-7 w-7 items-center justify-center rounded-lg
                  {backend.backend_type === 'embedded' ? 'bg-success/10' : 'bg-primary/10'}">
                  <Cpu class="h-3.5 w-3.5 {backend.backend_type === 'embedded' ? 'text-success' : 'text-primary'}" />
                </div>
                <div class="flex-1 min-w-0">
                  <p class="text-xs font-medium truncate">{backend.name}</p>
                  <p class="text-[10px] text-muted-foreground/60 truncate">{backend.model}</p>
                </div>
                {#if backend.status === "ready"}
                  <span class="h-2 w-2 rounded-full bg-success"></span>
                {:else if backend.status === "loading"}
                  <span class="h-2 w-2 rounded-full bg-warning animate-pulse"></span>
                {:else}
                  <span class="h-2 w-2 rounded-full bg-destructive"></span>
                {/if}
              </button>
            {/each}
          </div>
        {:else}
          <p class="text-xs text-muted-foreground/60 italic">{$t("chat.config_no_providers")}</p>
        {/if}
      </div>

      <!-- Instructions section -->
      <div class="glass-card glass-border rounded-xl p-3.5 space-y-2.5">
        <label class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/50" for="config-prompt">
          {$t("chat.system_prompt")}
        </label>
        <textarea
          id="config-prompt"
          class="w-full rounded-lg glass-surface glass-border border px-3.5 py-2.5 text-xs text-foreground
            bg-transparent resize-none outline-none transition-all
            focus:ring-2 focus:ring-primary/30 focus:border-primary/40
            placeholder:text-muted-foreground/50
            disabled:opacity-50 disabled:cursor-not-allowed"
          rows="4"
          placeholder={$t("chat.system_prompt_placeholder")}
          bind:value={systemPrompt}
          disabled={isReadOnly}
          data-testid="config-system-prompt"
        ></textarea>
      </div>

      <!-- Tools section (libre mode only) -->
      {#if session?.mode === "libre"}
        <div class="glass-card glass-border rounded-xl p-3.5 space-y-2.5">
          <p class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/50">
            {$t("chat.select_tools")}
          </p>
          <div class="space-y-1">
            {#each TOOL_META as tool (tool.id)}
              <div class="flex items-center justify-between rounded-lg px-3 py-2 glass-surface {isReadOnly ? 'opacity-50' : ''}">
                <div class="flex items-center gap-2.5">
                  {#if tool.id === "bash_executor"}
                    <Terminal class="h-3.5 w-3.5 text-muted-foreground" />
                  {:else if tool.id === "file_io"}
                    <FileText class="h-3.5 w-3.5 text-muted-foreground" />
                  {:else}
                    <Code class="h-3.5 w-3.5 text-muted-foreground" />
                  {/if}
                  <span class="text-xs font-medium">{tool.label}</span>
                </div>
                <Toggle
                  size="sm"
                  checked={getToolChecked(tool.id)}
                  onchange={(checked) => setToolChecked(tool.id, checked)}
                  disabled={isReadOnly}
                  data-testid="config-tool-{tool.id}"
                />
              </div>
            {/each}
          </div>
        </div>
      {/if}

      <!-- Save error -->
      {#if saveError}
        <div class="rounded-lg border border-destructive/30 bg-destructive/5 px-3.5 py-2.5 animate-fade-in">
          <p class="text-xs text-destructive" data-testid="config-error">{saveError}</p>
        </div>
      {/if}
    </div>

    <!-- Footer -->
    {#if !isReadOnly}
      <div class="border-t glass-border-subtle px-5 py-4">
        <Button
          class="w-full bg-primary"
          onclick={handleSave}
          disabled={saving}
          data-testid="config-save"
        >
          <Save class="mr-2 h-4 w-4" />
          {saving ? $t("common.submitting") : $t("chat.config_save")}
        </Button>
      </div>
    {/if}
  </div>
</Sheet>
