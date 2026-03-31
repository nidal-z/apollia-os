<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { Save, Cpu, Settings2, TriangleAlert, Brain, ChevronDown } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Toggle } from "$lib/components/ui/toggle";
  import { Sheet } from "$lib/components/ui/sheet";
  import type { ChatSessionDetail, UpdateSessionRequest } from "$lib/types";
  import { TOOL_GROUPS, TOOL_CATALOG, getGroupState, toggleGroup } from "$lib/tools/tool-catalog";
  import { llmBackends } from "$lib/stores/sse";
  import { useUserMemory, chatConversationStats } from "$lib/stores/chat";
  import { uiMode } from "$lib/stores/mode";

  interface Props {
    open: boolean;
    session: ChatSessionDetail | null;
    onclose: () => void;
    onupdated: () => void;
  }

  let { open, session, onclose, onupdated }: Props = $props();

  let systemPrompt = $state("");
  let enabledTools = $state(new Set<string>());
  /** Groups collapsed in the config panel. Default: all expanded. */
  let collapsedGroups = $state(new Set<string>());
  let selectedBackend = $state<string | null>(null);
  let saving = $state(false);
  let saveError = $state<string | null>(null);

  const isClosed = $derived(session?.status === "closed");
  const isProcessing = $derived(session?.status === "processing");
  const isReadOnly = $derived(isClosed || isProcessing);

  const selectedTools = $derived(Array.from(enabledTools));

  const memoryToggleLabel = $derived(
    $uiMode === "builder"
      ? $t("chat.inject_user_memory")
      : $t("chat.use_preferences"),
  );

  const contextBarSegments = $derived.by(() => {
    const stats = $chatConversationStats;
    if (!stats) return null;
    const pct = stats.context_usage_pct;
    const hasMemory = stats.user_memory_injected;
    const hasSummary = stats.summarized_count > 0;
    const memoryPct = hasMemory ? Math.min(pct * 0.15, 15) : 0;
    const summaryPct = hasSummary ? Math.min(pct * 0.1, 10) : 0;
    const messagesPct = Math.max(pct - memoryPct - summaryPct, 0);
    const freePct = Math.max(100 - messagesPct - memoryPct - summaryPct, 0);
    return { messagesPct, memoryPct, summaryPct, freePct };
  });

  function syncFromSession(): void {
    if (!session) return;
    systemPrompt = session.system_prompt ?? "";
    enabledTools = new Set(session.available_tools ?? []);
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

  const isOperator = $derived($uiMode === "operator");

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
                  {backend.provider === 'llama-cpp' ? 'bg-success/10' : 'bg-primary/10'}">
                  <Cpu class="h-3.5 w-3.5 {backend.provider === 'llama-cpp' ? 'text-success' : 'text-primary'}" />
                </div>
                <div class="flex-1 min-w-0">
                  <p class="text-xs font-medium truncate">{backend.name}</p>
                  <p class="text-[10px] text-muted-foreground/60 truncate">{backend.model}</p>
                </div>
                {#if backend.enabled}
                  <span class="h-2 w-2 rounded-full bg-success"></span>
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
        <div class="glass-card glass-border rounded-xl p-3.5 space-y-3">
          <p class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/50">
            {isOperator ? $t("tools.select.title_operator") : $t("tools.select.title_builder")}
          </p>

          {#each TOOL_GROUPS as group (group.id)}
            {@const groupTools = TOOL_CATALOG.filter(t => t.group === group.id)}
            {@const state = getGroupState(group, enabledTools)}
            {@const GroupIcon = group.icon}
            {@const isCollapsed = collapsedGroups.has(group.id)}
            {@const enabledCount = groupTools.filter(t => enabledTools.has(t.id)).length}
            <div class="space-y-0.5">
              <!-- Collapsible group header -->
              <div class="flex items-center justify-between px-1 py-1 rounded hover:bg-muted/20 transition-colors">
                <!-- Left: collapse toggle -->
                <button
                  class="flex items-center gap-1.5 flex-1 text-left"
                  onclick={() => {
                    const next = new Set(collapsedGroups);
                    if (next.has(group.id)) next.delete(group.id); else next.add(group.id);
                    collapsedGroups = next;
                  }}
                  data-testid="config-group-header-{group.id}"
                >
                  <GroupIcon class="h-3 w-3 text-muted-foreground/60" />
                  <span class="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground/60">
                    {isOperator ? $t(group.labelOperatorKey) : $t(group.labelBuilderKey)}
                  </span>
                  {#if isCollapsed}
                    <span class="text-[10px] text-muted-foreground/40 font-normal normal-case tracking-normal">
                      {state === "all" ? "· all" : state === "some" ? `· ${enabledCount}/${groupTools.length}` : "· none"}
                    </span>
                  {/if}
                  <ChevronDown
                    class="h-3 w-3 text-muted-foreground/30 transition-transform duration-150 ml-auto {isCollapsed ? '' : 'rotate-180'}"
                  />
                </button>
                <!-- Right: toggle all (shown when expanded) -->
                {#if !isCollapsed && !isReadOnly}
                  <button
                    class="text-[9px] text-muted-foreground/30 hover:text-primary transition-colors px-1.5 ml-1"
                    onclick={() => { enabledTools = toggleGroup(group, enabledTools); }}
                    data-testid="config-group-toggle-{group.id}"
                  >
                    {state === "all" ? "− none" : "+ all"}
                  </button>
                {/if}
              </div>

              <!-- Individual tools (hidden when collapsed) -->
              {#if !isCollapsed}
                {#each groupTools as tool (tool.id)}
                  {@const checked = enabledTools.has(tool.id)}
                  {@const ToolIcon = tool.icon}
                  <div class="flex items-center justify-between rounded-lg px-3 py-2 glass-surface {isReadOnly ? 'opacity-50' : ''}">
                    <div class="flex items-center gap-2.5 min-w-0 flex-1">
                      <ToolIcon class="h-3.5 w-3.5 shrink-0 {tool.dangerous ? 'text-amber-500/70' : 'text-muted-foreground'}" />
                      <div class="min-w-0">
                        <div class="flex items-center gap-1.5">
                          <span class="text-xs font-medium truncate">
                            {isOperator ? $t(tool.labelOperatorKey) : tool.id}
                          </span>
                          {#if tool.dangerous && !isOperator}
                            <TriangleAlert class="h-2.5 w-2.5 text-amber-500/60 shrink-0" />
                          {/if}
                        </div>
                        <p class="text-[10px] text-muted-foreground/50 leading-tight mt-0.5">
                          {isOperator ? $t(tool.descOperatorKey) : $t(tool.descBuilderKey)}
                        </p>
                      </div>
                    </div>
                    <Toggle
                      size="sm"
                      {checked}
                      onchange={(v) => {
                        const next = new Set(enabledTools);
                        if (v) next.add(tool.id); else next.delete(tool.id);
                        enabledTools = next;
                      }}
                      disabled={isReadOnly}
                      data-testid="config-tool-{tool.id}"
                    />
                  </div>
                {/each}
              {/if}
            </div>
          {/each}
        </div>
      {/if}

      <!-- User memory toggle -->
      <div class="glass-card glass-border rounded-xl p-3.5 space-y-2.5">
        <p class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/50">
          {$t("chat.memory_section")}
        </p>
        <div class="flex items-center justify-between rounded-lg px-3 py-2 glass-surface">
          <div class="flex items-center gap-2.5">
            <Brain class="h-3.5 w-3.5 text-[#7c5fd6]" />
            <span class="text-xs font-medium">{memoryToggleLabel}</span>
          </div>
          <Toggle
            size="sm"
            checked={$useUserMemory}
            onchange={(checked) => useUserMemory.set(checked)}
            data-testid="toggle-user-memory"
          />
        </div>
      </div>

      <!-- Context window usage -->
      {#if contextBarSegments}
        <div class="glass-card glass-border rounded-xl p-3.5 space-y-2.5">
          <p class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/50">
            {$t("chat.context_window")}
          </p>
          <div
            class="flex h-2 w-full overflow-hidden rounded-full bg-muted/20"
            data-testid="context-window-bar"
          >
            {#if contextBarSegments.messagesPct > 0}
              <div
                class="h-full bg-[#3435f5]"
                style="width: {contextBarSegments.messagesPct}%"
                title={$t("chat.context_messages", { values: { pct: Math.round(contextBarSegments.messagesPct) } })}
              ></div>
            {/if}
            {#if contextBarSegments.memoryPct > 0}
              <div
                class="h-full bg-[#7c5fd6]"
                style="width: {contextBarSegments.memoryPct}%"
                title={$t("chat.context_memory", { values: { pct: Math.round(contextBarSegments.memoryPct) } })}
              ></div>
            {/if}
            {#if contextBarSegments.summaryPct > 0}
              <div
                class="h-full bg-[#f59e0b]"
                style="width: {contextBarSegments.summaryPct}%"
                title={$t("chat.context_summary", { values: { pct: Math.round(contextBarSegments.summaryPct) } })}
              ></div>
            {/if}
            <div
              class="h-full bg-[#6b7280]/30 flex-1"
              title={$t("chat.context_free", { values: { pct: Math.round(contextBarSegments.freePct) } })}
            ></div>
          </div>
          <div class="flex items-center gap-3 text-[9px] text-muted-foreground/50">
            <span class="flex items-center gap-1">
              <span class="inline-block h-1.5 w-1.5 rounded-full bg-[#3435f5]"></span>
              {$t("chat.legend_messages")}
            </span>
            <span class="flex items-center gap-1">
              <span class="inline-block h-1.5 w-1.5 rounded-full bg-[#7c5fd6]"></span>
              {$t("chat.legend_memory")}
            </span>
            <span class="flex items-center gap-1">
              <span class="inline-block h-1.5 w-1.5 rounded-full bg-[#f59e0b]"></span>
              {$t("chat.legend_summary")}
            </span>
            <span class="flex items-center gap-1">
              <span class="inline-block h-1.5 w-1.5 rounded-full bg-[#6b7280]/30"></span>
              {$t("chat.legend_free")}
            </span>
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
