<script module lang="ts">
  /** How much of the notes the closed section shows. */
  const NOTES_EXCERPT_CHARS = 48;

  /** The start of the notes on one line, `null` when there are none. */
  export function notesExcerpt(notes: string): string | null {
    const line = notes.trim().replace(/\s+/g, " ");
    if (line === "") return null;
    return line.length > NOTES_EXCERPT_CHARS ? `${line.slice(0, NOTES_EXCERPT_CHARS)}…` : line;
  }

  /** Where the slider stands while no temperature is set anywhere. */
  export const TEMPERATURE_SLIDER_REST = 0.8;

  /**
   * What the conversation stores for the switch that keeps the reasoning of
   * the earlier answers: on is stored, off leaves the field out.
   */
  export function keptReasoning(on: boolean): true | undefined {
    return on ? true : undefined;
  }
</script>

<script lang="ts">
  /**
   * The Settings tab of the right panel of the chat: what holds for this
   * conversation alone. The preset, the instructions added to the mission of
   * the agent, the reasoning the model offers, the answer (temperature,
   * length, context limit), and the notes. Each control writes at once.
   */
  import { untrack } from "svelte";
  import { t } from "svelte-i18n";
  import { BookmarkPlus, UserCog, Undo2 } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { Disclosure } from "$lib/components/ui/disclosure";
  import { Input } from "$lib/components/ui/input";
  import { Select } from "$lib/components/ui/select";
  import { Textarea } from "$lib/components/ui/textarea";
  import { Toggle } from "$lib/components/ui/toggle";
  import { addToast } from "$lib/components/ui/toast/store";
  import type { ChatSessionDetail } from "$lib/types";
  import { getChatModelCapabilities } from "$lib/ipc/chat";
  import { listModelPresets } from "$lib/ipc/agentAdjust";
  import {
    createModelPreset,
    getChatSessionNotes,
    getChatSessionSettings,
    setChatSessionNotes,
    setChatSessionSettings,
  } from "$lib/ipc/chatSettings";
  import { APOLLIA_AGENT_ID } from "$lib/chat/chatTargets";
  import { chatPromptTemplate } from "$lib/stores/chatPromptTemplate";
  import { developerMode } from "$lib/stores/developerMode";
  import {
    CONTEXT_OVERFLOWS,
    CONTEXT_OVERFLOW_KEYS,
    DEFAULT_MAX_TOKENS,
    INSTRUCTIONS_MAX_CHARS,
    NOTES_MAX_CHARS,
    PRESET_FIELD_KEYS,
    REASONING_EFFORTS,
    REASONING_EFFORT_KEYS,
    agentIdOf,
    effectiveValue,
    instructionsTooLong,
    instructionsValue,
    maxTokensValue,
    temperatureValue,
    type ContextOverflow,
    type PresetField,
    type ReasoningEffort,
    type SessionPreset,
  } from "$lib/chat/sessionSettings";
  import {
    createSessionSettings,
    type SessionSettingsController,
  } from "./useSessionSettings.svelte";
  import PresetBlock from "./PresetBlock.svelte";
  import PromptTemplateBlock, { promptTemplateShown } from "./PromptTemplateBlock.svelte";
  import PresetNameDialog from "./PresetNameDialog.svelte";
  import ApplyToAgentDialog from "./ApplyToAgentDialog.svelte";
  import PresetSaveConfirmDialog, {
    holdPresetSave,
    type PendingPresetSave,
  } from "../../presets/PresetSaveConfirmDialog.svelte";
  import { presetSaveConfirm } from "$lib/stores/generalBehaviours";

  interface Props {
    session: ChatSessionDetail | null;
    /** The settings shown; the tab creates its own when left out. */
    controller?: SessionSettingsController;
  }

  let {
    session,
    controller = createSessionSettings({
      getSettings: getChatSessionSettings,
      setSettings: setChatSessionSettings,
      getNotes: getChatSessionNotes,
      setNotes: setChatSessionNotes,
      listPresets: async () => (await listModelPresets()) as SessionPreset[],
      createPreset: createModelPreset,
      capabilities: getChatModelCapabilities,
    }),
  }: Props = $props();

  let instructions = $state("");
  let instructionsTimer: ReturnType<typeof setTimeout> | null = null;
  let saving = $state(false);
  let pendingSave = $state<PendingPresetSave | null>(null);
  let applying = $state(false);

  const closed = $derived(session?.status === "closed");
  const settings = $derived(controller.settings);
  const temperature = $derived(effectiveValue(settings, controller.preset, "temperature"));
  const maxTokens = $derived(effectiveValue(settings, controller.preset, "max_tokens"));
  const overflow = $derived<ContextOverflow>(settings.context_overflow ?? "summarize_middle");
  const tooLong = $derived(instructionsTooLong(instructions));
  const agentId = $derived(session ? agentIdOf(session, APOLLIA_AGENT_ID) : APOLLIA_AGENT_ID);
  const agentName = $derived(
    session?.mode === "agent" && session.agent_name
      ? session.agent_name
      : $t("authorizations.apollia_name"),
  );
  const excerpt = $derived(notesExcerpt(controller.notes));

  function isModified(field: PresetField): boolean {
    return controller.modified.includes(field);
  }

  // The tab stays mounted across conversations: each one is read as it opens,
  // and read again when its model changes, since presets belong to a model.
  // What was being typed for the previous one is stored first.
  let loadedFor: string | null = null;
  $effect(() => {
    const id = session?.id ?? null;
    const key = id ? `${id}|${session?.llm_backend ?? ""}` : null;
    if (!id || key === loadedFor) return;
    loadedFor = key;
    untrack(() => {
      storeInstructions();
      void controller.load(id).then(() => {
        if (loadedFor === key) instructions = controller.settings.instructions ?? "";
      });
    });
  });

  $effect(() => {
    return () => {
      storeInstructions();
      void controller.flushNotes();
    };
  });

  function storeInstructions(): void {
    if (instructionsTimer) {
      clearTimeout(instructionsTimer);
      instructionsTimer = null;
    }
    if (tooLong || controller.loading || controller.sessionId === null) return;
    const value = instructionsValue(instructions);
    if (value === controller.settings.instructions) return;
    void controller.set("instructions", value);
  }

  function typeInstructions(text: string): void {
    instructions = text;
    if (instructionsTimer) clearTimeout(instructionsTimer);
    instructionsTimer = setTimeout(storeInstructions, 600);
  }

  // With the confirmation asked in Settings > General, the settings changed
  // are confirmed before the preset is named; a cancel never reaches the name.
  function askSaveAs(): void {
    pendingSave = holdPresetSave($presetSaveConfirm, {
      name: null,
      fieldKeys: controller.modified.map((field) => PRESET_FIELD_KEYS[field]),
      run: () => {
        saving = true;
      },
    });
  }

  async function saveAs(name: string): Promise<string | null> {
    const outcome = await controller.saveAsPreset(name);
    if (outcome.ok) {
      saving = false;
      addToast($t("chat.session_settings.preset.saved", { values: { name } }), "success");
      return null;
    }
    return outcome.reason === "refused"
      ? $t("chat.session_settings.preset.error_refused", { values: { reason: outcome.detail ?? "" } })
      : $t("chat.session_settings.preset.error_no_model");
  }
</script>

{#snippet modifiedMark(field: PresetField)}
  {#if isModified(field)}
    <span class="inline-flex items-center gap-1.5">
      <span
        class="inline-block h-1.5 w-1.5 rounded-full bg-warning"
        aria-hidden="true"
        data-testid="session-setting-modified-{field}"
      ></span>
      <button
        type="button"
        class="inline-flex items-center gap-1 text-micro text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 disabled:opacity-50"
        onclick={() => void controller.revert(field)}
        disabled={closed}
        data-testid="session-setting-revert-{field}"
      >
        <Undo2 size={10} aria-hidden="true" />
        {controller.preset
          ? $t("chat.session_settings.revert_to_preset")
          : $t("chat.session_settings.revert_to_default")}
      </button>
    </span>
  {/if}
{/snippet}

<div class="flex h-full min-h-0 flex-col" data-testid="chat-session-settings">
  <div class="min-h-0 flex-1 space-y-4 overflow-y-auto px-4 py-4">
    <PresetBlock {controller} disabled={closed} />

    {#if promptTemplateShown($chatPromptTemplate, $developerMode)}
      <PromptTemplateBlock {session} {controller} />
    {/if}

    <Disclosure open testid="session-instructions">
      {#snippet summary()}
        <span class="text-xs font-medium text-foreground">
          {$t("chat.session_settings.instructions.title")}
        </span>
      {/snippet}
      <div class="space-y-1.5 px-3 pb-3">
        <Textarea
          id="session-instructions-field"
          size="sm"
          rows={5}
          class="w-full resize-y"
          aria-label={$t("chat.session_settings.instructions.title")}
          aria-invalid={tooLong}
          aria-describedby="session-instructions-help"
          placeholder={$t("chat.session_settings.instructions.placeholder")}
          value={instructions}
          oninput={(e: Event) => typeInstructions((e.currentTarget as HTMLTextAreaElement).value)}
          onblur={storeInstructions}
          disabled={closed}
          data-testid="session-instructions-field"
        ></Textarea>
        <p
          id="session-instructions-help"
          class="text-micro {tooLong ? 'text-destructive' : 'text-muted-foreground'}"
        >
          <span data-testid="session-instructions-count">
            {$t("chat.session_settings.instructions.count", {
              values: {
                count: instructions.length.toLocaleString(),
                max: INSTRUCTIONS_MAX_CHARS.toLocaleString(),
              },
            })}
          </span>
          {tooLong
            ? $t("chat.session_settings.instructions.too_long")
            : $t("chat.session_settings.instructions.help")}
        </p>
      </div>
    </Disclosure>

    {#if controller.reasoning === "toggle" || controller.reasoning === "levels"}
      <Disclosure open testid="session-reasoning">
        {#snippet summary()}
          <span class="text-xs font-medium text-foreground">
            {$t("chat.session_settings.reasoning.title")}
          </span>
        {/snippet}
        <div class="space-y-1.5 px-3 pb-3">
          {#if controller.reasoning === "toggle"}
            <div class="flex items-center justify-between gap-3">
              <label for="session-thinking" class="text-xs text-foreground">
                {$t("chat.session_settings.reasoning.thinking")}
              </label>
              <Toggle
                id="session-thinking"
                size="sm"
                checked={settings.thinking ?? true}
                onchange={(on) => void controller.set("thinking", on)}
                disabled={closed}
                data-testid="session-thinking"
              />
            </div>
          {:else}
            <div class="flex items-center justify-between gap-3">
              <label for="session-reasoning-effort" class="text-xs text-foreground">
                {$t("chat.session_settings.reasoning.effort")}
              </label>
              <Select
                id="session-reasoning-effort"
                size="sm"
                class="w-36"
                value={settings.reasoning_effort ?? ""}
                onchange={(e: Event) => {
                  const raw = (e.currentTarget as HTMLSelectElement).value;
                  void controller.set(
                    "reasoning_effort",
                    raw === "" ? undefined : (raw as ReasoningEffort),
                  );
                }}
                disabled={closed}
                data-testid="session-reasoning-effort"
              >
                <option value="">{$t("chat.session_settings.reasoning.effort_default")}</option>
                {#each REASONING_EFFORTS as effort (effort)}
                  <option value={effort}>
                    {$t(REASONING_EFFORT_KEYS[effort])}
                  </option>
                {/each}
              </Select>
            </div>
          {/if}
          <div class="flex items-center justify-between gap-3">
            <label for="session-preserve-thinking" class="text-xs text-foreground">
              {$t("chat.session_settings.reasoning.preserve_thinking")}
            </label>
            <Toggle
              id="session-preserve-thinking"
              size="sm"
              checked={settings.preserve_thinking ?? false}
              onchange={(on) => void controller.set("preserve_thinking", keptReasoning(on))}
              disabled={closed}
              data-testid="session-preserve-thinking"
            />
          </div>
          <p class="text-micro text-muted-foreground" data-testid="session-preserve-thinking-help">
            {$t("chat.session_settings.reasoning.preserve_thinking_help")}
          </p>
          <p class="text-micro text-muted-foreground">
            {$t("chat.session_settings.reasoning.scope")}
          </p>
        </div>
      </Disclosure>
    {/if}

    <Disclosure testid="session-response">
      {#snippet summary()}
        <span class="flex min-w-0 flex-col">
          <span class="text-xs font-medium text-foreground">
            {$t("chat.session_settings.response.title")}
          </span>
          <span class="truncate text-micro text-muted-foreground" data-testid="session-response-summary">
            {$t("chat.session_settings.response.summary", {
              values: {
                temperature:
                  temperature === undefined
                    ? $t("chat.session_settings.response.by_default")
                    : temperature,
                length:
                  maxTokens === undefined
                    ? $t("chat.session_settings.response.length_free")
                    : maxTokens,
                overflow: $t(CONTEXT_OVERFLOW_KEYS[overflow].label),
              },
            })}
          </span>
        </span>
      {/snippet}
      <div class="space-y-4 px-3 pb-3">
        <div class="space-y-1.5">
          <div class="flex items-center justify-between gap-2">
            <label for="session-temperature" class="text-xs text-foreground">
              {$t("chat.session_settings.response.temperature")}
            </label>
            {@render modifiedMark("temperature")}
          </div>
          <div class="flex items-center gap-3">
            <input
              type="range"
              min="0"
              max="2"
              step="0.05"
              value={temperature ?? TEMPERATURE_SLIDER_REST}
              onchange={(e) =>
                void controller.set("temperature", temperatureValue(e.currentTarget.value))}
              aria-label={$t("chat.session_settings.response.temperature")}
              class="h-1 min-w-0 flex-1 cursor-pointer accent-primary disabled:cursor-not-allowed disabled:opacity-50"
              disabled={closed}
              data-testid="session-temperature-slider"
            />
            <Input
              id="session-temperature"
              type="number"
              size="sm"
              min="0"
              max="2"
              step="0.05"
              class="w-20"
              placeholder={$t("chat.session_settings.response.by_default")}
              value={temperature ?? ""}
              onchange={(e: Event) =>
                void controller.set(
                  "temperature",
                  temperatureValue((e.currentTarget as HTMLInputElement).value),
                )}
              disabled={closed}
              data-testid="session-temperature"
            />
          </div>
          <p class="text-micro text-muted-foreground">
            {$t("chat.session_settings.response.temperature_hint")}
          </p>
        </div>

        <div class="space-y-1.5">
          <div class="flex items-center justify-between gap-2">
            <div class="flex items-center gap-2.5">
              <Checkbox
                id="session-limit-length"
                checked={maxTokens !== undefined}
                onchange={(on) =>
                  void controller.set("max_tokens", on ? DEFAULT_MAX_TOKENS : undefined)}
                disabled={closed || (settings.max_tokens === undefined && maxTokens !== undefined)}
                data-testid="session-limit-length"
              />
              <label for="session-limit-length" class="text-xs text-foreground">
                {$t("chat.session_settings.response.limit_length")}
              </label>
            </div>
            {@render modifiedMark("max_tokens")}
          </div>
          {#if maxTokens !== undefined}
            <div class="flex items-center gap-2">
              <Input
                id="session-max-tokens"
                type="number"
                size="sm"
                min="1"
                step="1"
                class="w-28"
                aria-label={$t("chat.session_settings.response.limit_length")}
                value={maxTokens}
                onchange={(e: Event) => {
                  const value = maxTokensValue((e.currentTarget as HTMLInputElement).value);
                  if (value !== undefined) void controller.set("max_tokens", value);
                }}
                disabled={closed}
                data-testid="session-max-tokens"
              />
              <span class="text-micro text-muted-foreground">
                {$t("chat.session_settings.response.length_unit")}
              </span>
            </div>
          {/if}
        </div>

        <div class="space-y-1.5">
          <label for="session-context-overflow" class="text-xs text-foreground">
            {$t("chat.session_settings.response.overflow")}
          </label>
          <Select
            id="session-context-overflow"
            size="sm"
            value={overflow}
            onchange={(e: Event) => {
              const raw = (e.currentTarget as HTMLSelectElement).value as ContextOverflow;
              void controller.set(
                "context_overflow",
                raw === "summarize_middle" ? undefined : raw,
              );
            }}
            disabled={closed}
            data-testid="session-context-overflow"
          >
            {#each CONTEXT_OVERFLOWS as value (value)}
              <option {value}>{$t(CONTEXT_OVERFLOW_KEYS[value].label)}</option>
            {/each}
          </Select>
          <p class="text-micro text-muted-foreground">
            {$t(CONTEXT_OVERFLOW_KEYS[overflow].help)}
          </p>
        </div>
      </div>
    </Disclosure>

    <Disclosure testid="session-notes">
      {#snippet summary()}
        <span class="flex min-w-0 flex-col">
          <span class="text-xs font-medium text-foreground">
            {$t("chat.session_settings.notes.title")}
          </span>
          <span class="truncate text-micro text-muted-foreground" data-testid="session-notes-summary">
            {excerpt ?? $t("chat.session_settings.notes.empty")}
          </span>
        </span>
      {/snippet}
      <div class="space-y-1.5 px-3 pb-3">
        <Textarea
          id="session-notes-field"
          size="sm"
          rows={6}
          class="w-full resize-y"
          aria-label={$t("chat.session_settings.notes.title")}
          maxlength={NOTES_MAX_CHARS}
          placeholder={$t("chat.session_settings.notes.placeholder")}
          value={controller.notes}
          oninput={(e: Event) =>
            controller.typeNotes((e.currentTarget as HTMLTextAreaElement).value)}
          onblur={() => void controller.flushNotes()}
          disabled={closed}
          data-testid="session-notes-field"
        ></Textarea>
        <p
          class="text-micro text-muted-foreground"
          data-testid="session-notes-status"
          data-saved={controller.notesSaved ? "true" : "false"}
        >
          {controller.notesSaved
            ? $t("chat.session_settings.notes.saved")
            : $t("chat.session_settings.notes.saving")}
        </p>
      </div>
    </Disclosure>

    {#if controller.error}
      <p
        class="rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive"
        role="alert"
        data-testid="session-settings-error"
      >
        {controller.error}
      </p>
    {/if}
  </div>

  {#if !closed}
    <div class="space-y-2 border-t border-border px-4 py-3">
      <Button
        variant="ghost"
        size="sm"
        class="w-full justify-start"
        onclick={askSaveAs}
        disabled={controller.model === null}
        data-testid="session-save-as-preset"
      >
        {#snippet icon()}<BookmarkPlus size={14} />{/snippet}
        {$t("chat.session_settings.preset.save_as")}
      </Button>
      <Button
        variant="primary-solid"
        size="sm"
        class="w-full"
        onclick={() => (applying = true)}
        data-testid="session-apply-to-agent"
      >
        {#snippet icon()}<UserCog size={14} />{/snippet}
        {$t("chat.session_settings.apply.action")}
      </Button>
    </div>
  {/if}
</div>

<PresetNameDialog
  open={saving}
  title={$t("chat.session_settings.preset.save_as")}
  onclose={() => (saving = false)}
  onsubmit={saveAs}
  testid="session-save-preset-dialog"
/>

<PresetSaveConfirmDialog
  pending={pendingSave}
  onsettled={() => (pendingSave = null)}
  testid="session-save-preset-confirm"
/>

<ApplyToAgentDialog
  open={applying}
  {agentId}
  {agentName}
  {controller}
  onclose={() => (applying = false)}
  onapplied={() =>
    addToast($t("chat.session_settings.apply.done", { values: { agent: agentName } }), "success")}
/>
