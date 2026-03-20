<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { AgentListItem, CreateSessionRequest, ChatSessionSummary } from "$lib/types";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    open: boolean;
    onclose: () => void;
    oncreated: (sessionId: string) => void;
    initialMode?: "libre" | "agent";
    initialAgent?: string;
  }

  let { open, onclose, oncreated, initialMode, initialAgent }: Props = $props();

  type ChatMode = "libre" | "agent";

  let mode = $state<ChatMode>("libre");
  let agents = $state<AgentListItem[]>([]);
  let selectedAgent = $state("");
  let systemPrompt = $state("");
  let submitting = $state(false);
  let submitError = $state<string | null>(null);

  let toolBash = $state(false);
  let toolFileIo = $state(false);
  let toolPython = $state(false);

  const activeAgents = $derived(
    agents.filter((a) => a.runtime_status === "active"),
  );

  const selectedTools = $derived.by(() => {
    const tools: string[] = [];
    if (toolBash) tools.push("bash_executor");
    if (toolFileIo) tools.push("file_io");
    if (toolPython) tools.push("python_executor");
    return tools;
  });

  const isValid = $derived.by(() => {
    if (mode === "agent") return !!selectedAgent;
    return true;
  });

  async function handleSubmit(): Promise<void> {
    if (!isValid) return;

    submitting = true;
    submitError = null;
    try {
      const request: CreateSessionRequest = { mode };

      if (mode === "agent") {
        request.agent_name = selectedAgent;
      } else if (selectedTools.length > 0) {
        request.tools = selectedTools;
      }

      if (systemPrompt.trim()) {
        request.system_prompt = systemPrompt.trim();
      }

      const session = await invoke<ChatSessionSummary>("create_chat_session", { request });
      oncreated(session.id);
      onclose();
    } catch (err: unknown) {
      submitError = err instanceof Error ? err.message : String(err);
    } finally {
      submitting = false;
    }
  }

  function resetForm() {
    mode = initialMode ?? "libre";
    selectedAgent = initialAgent ?? "";
    systemPrompt = "";
    toolBash = false;
    toolFileIo = false;
    toolPython = false;
    submitting = false;
    submitError = null;
  }

  async function loadAgents(): Promise<void> {
    try {
      agents = await invoke("list_agents");
    } catch {
      agents = [];
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      onclose();
    }
  }

  $effect(() => {
    if (open) {
      resetForm();
      void loadAgents();
    }
  });
</script>

{#if open}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
    role="button"
    tabindex="-1"
    onclick={onclose}
    onkeydown={handleKeydown}
  >
    <div
      class="max-h-[85vh] w-[480px] overflow-y-auto rounded-lg border bg-background p-6 shadow-lg"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={handleKeydown}
      data-testid="new-chat-dialog"
    >
      <h3 class="mb-4 text-lg font-semibold">{$t("chat.new_chat")}</h3>

      <div class="space-y-4">
        <!-- Mode selection -->
        <div>
          <label class="mb-1 block text-sm font-medium">{$t("chat.select_mode")}</label>
          <div class="flex gap-3">
            <label class="flex items-center gap-1.5 text-sm">
              <input
                type="radio"
                name="chat-mode"
                value="libre"
                bind:group={mode}
                data-testid="chat-mode-libre"
              />
              {$t("chat.mode_libre")}
            </label>
            <label class="flex items-center gap-1.5 text-sm">
              <input
                type="radio"
                name="chat-mode"
                value="agent"
                bind:group={mode}
                data-testid="chat-mode-agent"
              />
              {$t("chat.mode_agent")}
            </label>
          </div>
        </div>

        <!-- Agent select (mode Agent) -->
        {#if mode === "agent"}
          <div>
            <label class="mb-1 block text-sm font-medium" for="chat-agent-select">{$t("chat.select_agent")}</label>
            <select
              id="chat-agent-select"
              class="w-full rounded-md border bg-background px-3 py-2 text-sm"
              bind:value={selectedAgent}
              data-testid="chat-agent-select"
            >
              <option value="" disabled>-- {$t("chat.select_agent")} --</option>
              {#each activeAgents as agent}
                <option value={agent.name}>{agent.name}</option>
              {/each}
            </select>
            {#if activeAgents.length === 0}
              <p class="mt-0.5 text-xs text-muted-foreground">{$t("chat.no_active_agents")}</p>
            {/if}
          </div>
        {/if}

        <!-- Tool checkboxes (mode Libre) -->
        {#if mode === "libre"}
          <div>
            <label class="mb-1 block text-sm font-medium">{$t("chat.select_tools")}</label>
            <div class="flex flex-col gap-2">
              <label class="flex items-center gap-1.5 text-sm">
                <input type="checkbox" bind:checked={toolBash} data-testid="chat-tool-bash_executor" />
                bash_executor
              </label>
              <label class="flex items-center gap-1.5 text-sm">
                <input type="checkbox" bind:checked={toolFileIo} data-testid="chat-tool-file_io" />
                file_io
              </label>
              <label class="flex items-center gap-1.5 text-sm">
                <input type="checkbox" bind:checked={toolPython} data-testid="chat-tool-python_executor" />
                python_executor
              </label>
            </div>
            <p class="mt-0.5 text-xs text-muted-foreground">{$t("chat.tools_optional")}</p>
          </div>
        {/if}

        <!-- System prompt -->
        <div>
          <label class="mb-1 block text-sm font-medium" for="chat-system-prompt">
            {$t("chat.system_prompt")}
            <span class="font-normal text-muted-foreground">({$t("chat.optional")})</span>
          </label>
          <textarea
            id="chat-system-prompt"
            class="w-full rounded-md border bg-background px-3 py-2 text-sm"
            rows="3"
            placeholder={$t("chat.system_prompt_placeholder")}
            bind:value={systemPrompt}
            data-testid="chat-system-prompt"
          ></textarea>
        </div>

        <!-- Submit error -->
        {#if submitError}
          <p class="text-sm text-[hsl(var(--destructive))]" data-testid="new-chat-error">{submitError}</p>
        {/if}

        <!-- Actions -->
        <div class="flex justify-end gap-2">
          <Button variant="outline" size="sm" onclick={onclose} data-testid="chat-cancel-button">
            {$t("common.cancel")}
          </Button>
          <Button
            size="sm"
            onclick={handleSubmit}
            disabled={submitting || !isValid}
            data-testid="chat-start-button"
          >
            {submitting ? $t("common.submitting") : $t("chat.start")}
          </Button>
        </div>
      </div>
    </div>
  </div>
{/if}
