<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { X, Terminal, FileText, Code, Save, Cpu } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import type { ChatSessionDetail, LlmBackendStatus, UpdateSessionRequest } from "$lib/types";
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

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") onclose();
  }

  $effect(() => {
    if (open) syncFromSession();
  });
</script>

{#if open}
  <!-- Backdrop -->
  <div
    class="fixed inset-0 z-40 bg-black/30 backdrop-blur-sm"
    role="button"
    tabindex="-1"
    onclick={onclose}
    onkeydown={handleKeydown}
  ></div>

  <!-- Side panel -->
  <div
    class="fixed inset-y-0 right-0 z-50 flex w-[360px] flex-col glass-panel border-l glass-border shadow-lg animate-slide-in-right"
    role="dialog"
    aria-modal="true"
    tabindex="-1"
    onkeydown={handleKeydown}
    data-testid="chat-config-panel"
  >
    <!-- Header -->
    <div class="flex items-center justify-between border-b glass-border-subtle px-5 py-4">
      <h3 class="text-sm font-semibold tracking-wide uppercase text-foreground/80">
        {$t("chat.config_title")}
      </h3>
      <button
        onclick={onclose}
        class="inline-flex h-8 w-8 items-center justify-center rounded-lg glass-inset text-muted-foreground transition-all hover:text-foreground hover:scale-105 active:scale-95"
        data-testid="chat-config-close"
      >
        <X class="h-4 w-4" />
      </button>
    </div>

    <!-- Body -->
    <div class="flex-1 overflow-y-auto px-5 py-5 space-y-6">
      <!-- LLM Provider -->
      <div class="space-y-2.5">
        <label class="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
          {$t("chat.config_provider")}
        </label>
        {#if $llmBackends.length > 0}
          <div class="space-y-1.5">
            {#each $llmBackends as backend (backend.name)}
              <button
                class="w-full flex items-center gap-3 rounded-xl px-4 py-2.5 text-left transition-all
                  {(selectedBackend === backend.name || (selectedBackend === null && backend.name === $llmBackends[0]?.name))
                    ? 'glass-card ring-2 ring-primary/40 text-foreground'
                    : 'glass-inset text-muted-foreground hover:glass-surface hover:text-foreground'}"
                disabled={isReadOnly}
                onclick={() => (selectedBackend = backend.name)}
                data-testid="config-backend-{backend.name}"
              >
                <div class="flex h-7 w-7 items-center justify-center rounded-lg
                  {backend.backend_type === 'embedded' ? 'bg-[hsl(var(--success))]/10' : 'bg-primary/10'}">
                  <Cpu class="h-3.5 w-3.5 {backend.backend_type === 'embedded' ? 'text-[hsl(var(--success))]' : 'text-primary'}" />
                </div>
                <div class="flex-1 min-w-0">
                  <p class="text-sm font-medium truncate">{backend.name}</p>
                  <p class="text-[10px] text-muted-foreground/60 truncate">{backend.model}</p>
                </div>
                {#if backend.status === "ready"}
                  <span class="h-2 w-2 rounded-full bg-[hsl(var(--success))]"></span>
                {:else if backend.status === "loading"}
                  <span class="h-2 w-2 rounded-full bg-[hsl(var(--warning))] animate-pulse"></span>
                {:else}
                  <span class="h-2 w-2 rounded-full bg-[hsl(var(--destructive))]"></span>
                {/if}
              </button>
            {/each}
          </div>
        {:else}
          <p class="text-sm text-muted-foreground/60 italic">{$t("chat.config_no_providers")}</p>
        {/if}
      </div>

      <!-- Instructions -->
      <div class="space-y-2.5">
        <label class="text-xs font-semibold uppercase tracking-wider text-muted-foreground" for="config-prompt">
          {$t("chat.system_prompt")}
        </label>
        <textarea
          id="config-prompt"
          class="w-full rounded-xl glass-surface glass-border border px-4 py-3 text-sm text-foreground
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

      <!-- Tools -->
      {#if session?.mode === "libre"}
        <div class="space-y-2.5">
          <label class="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
            {$t("chat.select_tools")}
          </label>
          <div class="space-y-1.5">
            {#each TOOL_META as tool (tool.id)}
              {@const checked = tool.id === "bash_executor" ? toolBash : tool.id === "file_io" ? toolFileIo : toolPython}
              <label
                class="flex items-center gap-3 rounded-xl px-4 py-2.5 cursor-pointer transition-all
                  {checked ? 'glass-card ring-1 ring-primary/30' : 'glass-inset hover:glass-surface'}
                  {isReadOnly ? 'opacity-50 pointer-events-none' : ''}"
              >
                <div class="relative flex items-center">
                  <input
                    type="checkbox"
                    checked={checked}
                    onchange={(e) => {
                      const target = e.target as HTMLInputElement;
                      if (tool.id === "bash_executor") toolBash = target.checked;
                      else if (tool.id === "file_io") toolFileIo = target.checked;
                      else toolPython = target.checked;
                    }}
                    class="peer sr-only"
                    disabled={isReadOnly}
                    data-testid="config-tool-{tool.id}"
                  />
                  <div class="h-5 w-5 rounded-md border-2 glass-border transition-all
                    peer-checked:bg-primary peer-checked:border-primary">
                  </div>
                  <svg class="absolute left-[3px] top-[3px] h-3.5 w-3.5 text-white opacity-0 peer-checked:opacity-100 transition-opacity" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="3">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
                  </svg>
                </div>
                <svelte:component this={tool.icon} class="h-4 w-4 text-muted-foreground" />
                <span class="text-sm font-medium">{tool.label}</span>
              </label>
            {/each}
          </div>
        </div>
      {/if}

      <!-- Save error -->
      {#if saveError}
        <div class="rounded-xl border border-destructive/30 bg-destructive/5 px-4 py-3 animate-fade-in">
          <p class="text-sm text-[hsl(var(--destructive))]" data-testid="config-error">{saveError}</p>
        </div>
      {/if}
    </div>

    <!-- Footer -->
    {#if !isReadOnly}
      <div class="border-t glass-border-subtle px-5 py-4">
        <Button
          class="w-full"
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
{/if}

<style>
  .animate-slide-in-right {
    animation: slideInRight 0.25s cubic-bezier(0.16, 1, 0.3, 1);
  }
  @keyframes slideInRight {
    from { transform: translateX(100%); opacity: 0.8; }
    to { transform: translateX(0); opacity: 1; }
  }
</style>
