<script lang="ts">
  /**
   * The selector of a conversation: one button that shows who the
   * conversation talks to (an agent and its model, or Apollia and a model),
   * and opens one list, searched as it is typed, of the agents then the
   * models. Cmd L on macOS, Ctrl L elsewhere, opens it from the chat.
   *
   * It renders and reports the choice; what the choice does to the
   * conversation is `targetAction` in `$lib/chat/chatTargets`, applied by the
   * owner of the conversation.
   */
  import { onMount, tick } from "svelte";
  import { t } from "svelte-i18n";
  import { Bot, ChevronDown, CircleHelp, MessageSquare, Search } from "lucide-svelte";
  import { Tooltip } from "$lib/components/ui/tooltip";
  import { MOD_LABEL } from "$lib/shortcuts/chatShortcuts";
  import { chatTargetSelectorRequests } from "$lib/stores/chatTargetSelector";
  import { listChatTargets } from "$lib/ipc/chatTargets";
  import {
    buildTargetGroups,
    filterTargetGroups,
    modelLabel,
    targetLabel,
    type ChatTarget,
    type ConversationTarget,
    type TargetGroups,
  } from "$lib/chat/chatTargets";

  interface Props {
    /** The conversation shown, `null` before one is started. */
    conversation: ConversationTarget | null;
    /** Called with the target the user chose. */
    onpick: (target: ChatTarget) => void;
    /** Greys the button out, e.g. while a turn runs. */
    disabled?: boolean;
  }

  let { conversation, onpick, disabled = false }: Props = $props();

  let open = $state(false);
  let query = $state("");
  let loading = $state(false);
  let groups = $state<TargetGroups>({ agents: [], models: [] });
  let unreachable = $state<string[]>([]);
  let defaultModel = $state<string | null>(null);
  let root = $state<HTMLDivElement | undefined>(undefined);
  let searchInput = $state<HTMLInputElement | undefined>(undefined);

  const apolliaName = $derived(groups.agents.find((a) => a.builtin)?.name ?? "Apollia");
  const label = $derived(targetLabel(groups, conversation, defaultModel, apolliaName));
  const shown = $derived(filterTargetGroups(groups, query));
  const nothingShown = $derived(shown.agents.length === 0 && shown.models.length === 0);

  async function load(): Promise<void> {
    loading = true;
    try {
      const payload = await listChatTargets();
      groups = buildTargetGroups(payload);
      unreachable = payload.servers.filter((s) => s.error !== null).map((s) => s.server);
      const fallback = payload.default_model?.model_id;
      defaultModel = typeof fallback === "string" ? fallback : null;
    } catch {
      // The list stays as it was; the button still shows the conversation.
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void load();
  });

  // The chat shortcut: each request toggles the selector. The value the
  // store holds when the selector mounts is not a request.
  let seenRequests: number | null = null;
  $effect(() => {
    const requests = $chatTargetSelectorRequests;
    if (seenRequests === null) {
      seenRequests = requests;
      return;
    }
    if (requests === seenRequests) return;
    seenRequests = requests;
    void toggle();
  });

  async function toggle(): Promise<void> {
    if (open) {
      close();
      return;
    }
    if (disabled) return;
    open = true;
    query = "";
    void load();
    await tick();
    searchInput?.focus();
  }

  function close(): void {
    open = false;
    query = "";
  }

  function pick(target: ChatTarget): void {
    close();
    onpick(target);
  }

  function handleSearchKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      close();
    } else if (event.key === "Enter") {
      event.preventDefault();
      const first = shown.agents[0] ?? shown.models[0];
      if (first) pick(first);
    }
  }

  function handleWindowPointer(event: PointerEvent): void {
    if (!open || !root) return;
    if (event.target instanceof Node && root.contains(event.target)) return;
    close();
  }
</script>

<svelte:window onpointerdown={handleWindowPointer} />

<div class="relative min-w-0" bind:this={root} data-testid="chat-target-selector">
  <button
    type="button"
    onclick={() => void toggle()}
    {disabled}
    class="v4-hair inline-flex max-w-full items-center gap-1.5 rounded-md border bg-surface-1 px-2 py-1 text-body-xs text-foreground transition-colors hover:bg-surface-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 disabled:cursor-not-allowed disabled:opacity-50"
    aria-label={$t("chat.target_selector.aria")}
    aria-haspopup="listbox"
    aria-expanded={open}
    data-testid="chat-target-selector-button"
  >
    {#if label}
      <span class="truncate font-medium" data-testid="chat-target-selector-agent">{label.agent}</span>
      <span class="truncate text-muted-foreground" data-testid="chat-target-selector-model">
        {label.model ?? $t("chat.target_selector.default_model")}
      </span>
    {:else}
      <span class="truncate text-muted-foreground" data-testid="chat-target-selector-empty">
        {$t("chat.target_selector.placeholder")}
      </span>
    {/if}
    <kbd class="hidden items-center gap-0.5 text-overline text-muted-foreground sm:flex">
      <span class="v4-hair rounded border bg-muted px-1 py-px">{MOD_LABEL}</span>
      <span class="v4-hair rounded border bg-muted px-1 py-px">L</span>
    </kbd>
    <ChevronDown size={12} class="shrink-0 text-muted-foreground" />
  </button>

  {#if open}
    <div
      class="v4-hair absolute left-0 top-full z-overlay mt-1 flex max-h-96 w-80 flex-col rounded-lg border bg-card shadow-elev-2"
      role="dialog"
      aria-label={$t("chat.target_selector.aria")}
      data-testid="chat-target-selector-panel"
    >
      <div class="flex items-center gap-2 border-b border-border/40 px-2.5 py-2">
        <Search size={13} class="shrink-0 text-muted-foreground" />
        <input
          bind:this={searchInput}
          bind:value={query}
          onkeydown={handleSearchKeydown}
          type="text"
          class="w-full bg-transparent text-body-xs text-foreground outline-none placeholder:text-muted-foreground"
          placeholder={$t("chat.target_selector.search_placeholder")}
          aria-label={$t("chat.target_selector.search_aria")}
          data-testid="chat-target-selector-search"
        />
      </div>

      <div class="min-h-0 flex-1 overflow-y-auto p-1" role="listbox" aria-label={$t("chat.target_selector.aria")}>
        {#if loading && groups.agents.length === 0 && groups.models.length === 0}
          <p class="px-2 py-3 text-caption text-muted-foreground" data-testid="chat-target-selector-loading">
            {$t("chat.target_selector.loading")}
          </p>
        {:else if nothingShown}
          <p class="px-2 py-3 text-caption text-muted-foreground" data-testid="chat-target-selector-no-results">
            {$t("chat.target_selector.no_results")}
          </p>
        {/if}

        {#if shown.agents.length > 0}
          <p class="px-2 pb-1 pt-1.5 text-overline uppercase text-muted-foreground" data-testid="chat-target-selector-group-agents">
            {$t("chat.target_selector.group_agents")}
          </p>
          {#each shown.agents as agent (agent.id)}
            <button
              type="button"
              role="option"
              aria-selected="false"
              onclick={() => pick(agent)}
              class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-body-xs hover:bg-muted"
              data-testid="chat-target-selector-agent-option"
              data-target-id={agent.id}
            >
              {#if agent.builtin}
                <MessageSquare size={13} class="shrink-0 text-muted-foreground" />
              {:else}
                <Bot size={13} class="shrink-0 text-muted-foreground" />
              {/if}
              <span class="min-w-0 flex-1 truncate font-medium text-foreground">{agent.name}</span>
              <span class="max-w-40 truncate text-caption text-muted-foreground">
                {modelLabel(groups, agent.model) ?? $t("chat.target_selector.default_model")}
              </span>
            </button>
          {/each}
        {/if}

        {#if shown.models.length > 0}
          <div class="flex items-center gap-1 px-2 pb-1 pt-2">
            <p class="text-overline uppercase text-muted-foreground" data-testid="chat-target-selector-group-models">
              {$t("chat.target_selector.group_models")}
            </p>
            <Tooltip content={$t("chat.target_selector.formats_help")}>
              <span
                class="inline-flex text-muted-foreground"
                aria-label={$t("chat.target_selector.formats_help_aria")}
                data-testid="chat-target-selector-formats-help"
              >
                <CircleHelp size={11} />
              </span>
            </Tooltip>
          </div>
          {#each shown.models as model (model.route)}
            <button
              type="button"
              role="option"
              aria-selected="false"
              onclick={() => pick(model)}
              class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-body-xs hover:bg-muted"
              data-testid="chat-target-selector-model-option"
              data-target-route={model.route}
            >
              <span class="min-w-0 flex-1 truncate text-foreground">{model.label}</span>
              {#if model.parameters}
                <span class="text-caption text-muted-foreground">{model.parameters}</span>
              {/if}
              {#if model.format}
                <span
                  class="v4-hair rounded border px-1 py-px text-overline uppercase text-muted-foreground"
                  data-testid="chat-target-selector-format-badge"
                >
                  {model.format}
                </span>
              {:else if model.server}
                <span class="max-w-28 truncate text-caption text-muted-foreground">{model.server}</span>
              {/if}
            </button>
          {/each}
        {/if}

        {#each unreachable as server (server)}
          <p class="px-2 py-1 text-caption text-muted-foreground" data-testid="chat-target-selector-unreachable">
            {$t("chat.target_selector.server_unreachable", { values: { server } })}
          </p>
        {/each}
      </div>

      <p class="border-t border-border/40 px-2.5 py-2 text-caption text-muted-foreground" data-testid="chat-target-selector-behaviour">
        {$t("chat.target_selector.behaviour")}
      </p>
    </div>
  {/if}
</div>
