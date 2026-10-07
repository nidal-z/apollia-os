<script module lang="ts">
  import type { ChatAutoNaming } from "$lib/stores/chatAutoNaming";
  import type { SettingSource } from "$lib/ipc/settings";
  import { CHAT_BEHAVIOUR_KEYS, type ChatBehaviourKey } from "$lib/stores/chatBehaviours";

  /** The blocks of the section, in order, by the i18n key of their title. */
  export const CHAT_SETTINGS_BLOCKS = [
    "settings.chat.conversations_title",
    "settings.chat.shortcuts_title",
    "settings.chat.naming_title",
    "settings.chat.all_shortcuts_title",
  ] as const;

  /** The trash row of the Conversations block: its `data-testid` and texts. */
  export const CHAT_TRASH_ROW = {
    testId: "chat-trash-toggle",
    labelKey: "settings.chat.trash_label",
    hintKey: "settings.chat.trash_hint",
  } as const;

  /** A switch of the section written to `apollia.toml`, lockable by the policy. */
  export interface ChatBehaviourRow {
    testId: string;
    key: ChatBehaviourKey;
    labelKey: string;
    hintKey: string;
  }

  /** The token count row: a display preference, kept beside the theme. */
  export const CHAT_TOKEN_COUNT_ROW = {
    testId: "chat-token-count",
    labelKey: "settings.chat.token_count_label",
    hintKey: "settings.chat.token_count_hint",
  } as const;

  /**
   * The prompt template row: a display preference of the developer mode,
   * kept beside the theme.
   */
  export const CHAT_PROMPT_TEMPLATE_ROW = {
    testId: "chat-show-prompt-template",
    labelKey: "dev.chat.prompt_template.setting_label",
    hintKey: "dev.chat.prompt_template.setting_hint",
  } as const;

  /** A row of the Conversations block. */
  export type ChatConversationRow =
    | { kind: "behaviour"; row: ChatBehaviourRow }
    | { kind: "trash" }
    | { kind: "token_count" }
    | { kind: "prompt_template" }
    | { kind: "plan_mode" };

  /**
   * The rows of the Conversations block, in the order of the page: one empty
   * conversation, unloading before loading, the trash, the double click that
   * edits, the token count, the double click that renames, then the plan
   * mode.
   */
  export const CHAT_CONVERSATION_ROWS: readonly ChatConversationRow[] = [
    {
      kind: "behaviour",
      row: {
        testId: "chat-single-empty",
        key: CHAT_BEHAVIOUR_KEYS.singleEmpty,
        labelKey: "settings.chat.single_empty_label",
        hintKey: "settings.chat.single_empty_hint",
      },
    },
    {
      kind: "behaviour",
      row: {
        testId: "chat-unload-before-load",
        key: CHAT_BEHAVIOUR_KEYS.unloadBeforeLoad,
        labelKey: "settings.chat.unload_before_load_label",
        hintKey: "settings.chat.unload_before_load_hint",
      },
    },
    { kind: "trash" },
    {
      kind: "behaviour",
      row: {
        testId: "chat-double-click-edit",
        key: CHAT_BEHAVIOUR_KEYS.doubleClickEdit,
        labelKey: "settings.chat.double_click_edit_label",
        hintKey: "settings.chat.double_click_edit_hint",
      },
    },
    { kind: "token_count" },
    {
      kind: "behaviour",
      row: {
        testId: "chat-double-click-rename",
        key: CHAT_BEHAVIOUR_KEYS.doubleClickRename,
        labelKey: "settings.chat.double_click_rename_label",
        hintKey: "settings.chat.double_click_rename_hint",
      },
    },
    { kind: "plan_mode" },
  ];

  /**
   * The rows of the Conversations block as the page shows them: in developer
   * mode the prompt template row follows the token count; otherwise it is
   * not part of the page.
   */
  export function conversationRows(developerMode: boolean): readonly ChatConversationRow[] {
    if (!developerMode) return CHAT_CONVERSATION_ROWS;
    return CHAT_CONVERSATION_ROWS.flatMap((row): ChatConversationRow[] =>
      row.kind === "token_count" ? [row, { kind: "prompt_template" }] : [row],
    );
  }

  /** The `data-testid` of a row of the Conversations block. */
  export function conversationRowTestId(row: ChatConversationRow): string {
    if (row.kind === "behaviour") return row.row.testId;
    if (row.kind === "trash") return CHAT_TRASH_ROW.testId;
    if (row.kind === "prompt_template") return CHAT_PROMPT_TEMPLATE_ROW.testId;
    return row.kind === "token_count" ? CHAT_TOKEN_COUNT_ROW.testId : "plan-mode-default-toggle";
  }

  /** The rows of the keyboard shortcuts block, by `data-testid`. */
  export const CHAT_SHORTCUT_ROWS = ["chat-send-key", "chat-regenerate-shortcut"] as const;

  /** The label and explanation keys of the regeneration row, per platform. */
  export function regenerateRowKeys(mac: boolean): { labelKey: string; hintKey: string } {
    return mac
      ? {
          labelKey: "settings.chat.regenerate_label_mac",
          hintKey: "settings.chat.regenerate_hint_mac",
        }
      : {
          labelKey: "settings.chat.regenerate_label_other",
          hintKey: "settings.chat.regenerate_hint_other",
        };
  }

  /** The three naming choices, each with its explanation. */
  export const CHAT_NAMING_CHOICES: readonly {
    value: ChatAutoNaming;
    labelKey: string;
    hintKey: string;
  }[] = [
    {
      value: "never",
      labelKey: "settings.chat.naming_never",
      hintKey: "settings.chat.naming_never_hint",
    },
    {
      value: "auto",
      labelKey: "settings.chat.naming_auto",
      hintKey: "settings.chat.naming_auto_hint",
    },
    {
      value: "always",
      labelKey: "settings.chat.naming_always",
      hintKey: "settings.chat.naming_always_hint",
    },
  ];

  /** The label and explanation keys of the send key row, per platform. */
  export function sendKeyRowKeys(mac: boolean): { labelKey: string; hintKey: string } {
    return mac
      ? { labelKey: "settings.chat.send_mod_label_mac", hintKey: "settings.chat.send_mod_hint_mac" }
      : {
          labelKey: "settings.chat.send_mod_label_other",
          hintKey: "settings.chat.send_mod_hint_other",
        };
  }

  /** A naming the policy imposes locks the three choices. */
  export function namingLocked(source: SettingSource): boolean {
    return source === "policy";
  }
</script>

<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { Keyboard, Lock, MessageSquare, Tag } from "lucide-svelte";
  import { planModeDefault } from "$lib/stores/planModeSetting";
  import { chatSendKey } from "$lib/stores/chatSendKey";
  import { chatTrashEnabled } from "$lib/stores/chatTrash";
  import { chatTokenCount } from "$lib/stores/chatTokenCount";
  import { chatPromptTemplate } from "$lib/stores/chatPromptTemplate";
  import { developerMode } from "$lib/stores/developerMode";
  import {
    chatBehaviours,
    hydrateChatBehaviours,
    setChatBehaviour,
  } from "$lib/stores/chatBehaviours";
  import { addToast } from "$lib/components/ui/toast";
  import {
    chatAutoNaming,
    chatAutoNamingSource,
    hydrateChatAutoNaming,
    isChatAutoNaming,
    setChatAutoNaming,
  } from "$lib/stores/chatAutoNaming";
  import { isMacPlatform } from "$lib/navigation/shortcuts";
  import { Toggle } from "$lib/components/ui/toggle";
  import { RadioGroup, RadioItem } from "$lib/components/ui/radio";
  import SettingsSubPage from "../../components/settings/SettingsSubPage.svelte";
  import SettingsSection from "../../components/settings/SettingsSection.svelte";
  import SettingsFieldRow from "../../components/settings/SettingsFieldRow.svelte";
  import SettingsToggle from "../../components/settings/SettingsToggle.svelte";
  import ShortcutCatalog from "../../components/settings/shortcuts/ShortcutCatalog.svelte";

  const sendKeys = sendKeyRowKeys(isMacPlatform());
  const regenerateKeys = regenerateRowKeys(isMacPlatform());
  const regenerate = $derived($chatBehaviours[CHAT_BEHAVIOUR_KEYS.regenerateShortcut]);
  const locked = $derived(namingLocked($chatAutoNamingSource));

  onMount(() => {
    void hydrateChatAutoNaming();
    void hydrateChatBehaviours();
  });

  async function writeBehaviour(key: ChatBehaviourKey, next: boolean): Promise<void> {
    const written = await setChatBehaviour(key, next);
    if (!written) addToast($t("settings.chat.write_failed"), "error");
  }

  function chooseNaming(value: string): void {
    if (locked || !isChatAutoNaming(value)) return;
    void setChatAutoNaming(value);
  }
</script>

<SettingsSubPage route="chat" data-testid="chat-settings-section">
  <SettingsSection title={$t(CHAT_SETTINGS_BLOCKS[0])} bodyLayout="flush">
    {#snippet icon()}<MessageSquare size={15} strokeWidth={1.75} />{/snippet}
    {#each conversationRows($developerMode) as entry (conversationRowTestId(entry))}
      {#if entry.kind === "behaviour"}
        {@const held = $chatBehaviours[entry.row.key]}
        <div class="border-b border-border/60 py-1.5">
          <SettingsToggle
            id={`${entry.row.testId}-switch`}
            label={$t(entry.row.labelKey)}
            description={$t(entry.row.hintKey)}
            checked={held.value}
            source={held.source}
            onChange={(next) => void writeBehaviour(entry.row.key, next)}
            data-testid={entry.row.testId}
          />
        </div>
      {:else if entry.kind === "token_count"}
        <div class="border-b border-border/60 py-1.5">
          <SettingsToggle
            id={`${CHAT_TOKEN_COUNT_ROW.testId}-switch`}
            label={$t(CHAT_TOKEN_COUNT_ROW.labelKey)}
            description={$t(CHAT_TOKEN_COUNT_ROW.hintKey)}
            checked={$chatTokenCount}
            onChange={(next) => chatTokenCount.set(next)}
            data-testid={CHAT_TOKEN_COUNT_ROW.testId}
          />
        </div>
      {:else if entry.kind === "prompt_template"}
        <div class="border-b border-border/60 py-1.5">
          <SettingsToggle
            id={`${CHAT_PROMPT_TEMPLATE_ROW.testId}-switch`}
            label={$t(CHAT_PROMPT_TEMPLATE_ROW.labelKey)}
            description={$t(CHAT_PROMPT_TEMPLATE_ROW.hintKey)}
            checked={$chatPromptTemplate}
            onChange={(next) => chatPromptTemplate.set(next)}
            data-testid={CHAT_PROMPT_TEMPLATE_ROW.testId}
          />
        </div>
      {:else if entry.kind === "trash"}
        <SettingsFieldRow
          label={$t(CHAT_TRASH_ROW.labelKey)}
          hint={$t(CHAT_TRASH_ROW.hintKey)}
          for="chat-trash"
        >
          {#snippet control()}
            <Toggle
              id="chat-trash"
              checked={$chatTrashEnabled}
              onchange={(next) => chatTrashEnabled.set(next)}
              aria-label={$t(CHAT_TRASH_ROW.labelKey)}
              data-testid={CHAT_TRASH_ROW.testId}
            />
          {/snippet}
        </SettingsFieldRow>
      {:else}
        <SettingsFieldRow
          label={$t("settings.planMode.title")}
          hint={$t("settings.planMode.description")}
          for="plan-mode-default"
          border={false}
        >
          {#snippet control()}
            <Toggle
              id="plan-mode-default"
              checked={$planModeDefault}
              onchange={(next) => planModeDefault.set(next)}
              aria-label={$t("settings.planMode.title")}
              data-testid="plan-mode-default-toggle"
            />
          {/snippet}
        </SettingsFieldRow>
      {/if}
    {/each}
  </SettingsSection>

  <SettingsSection title={$t(CHAT_SETTINGS_BLOCKS[1])}>
    {#snippet icon()}<Keyboard size={15} strokeWidth={1.75} />{/snippet}
    <SettingsToggle
      id="chat-send-key"
      label={$t(sendKeys.labelKey)}
      description={$t(sendKeys.hintKey)}
      checked={$chatSendKey === "mod-enter"}
      onChange={(next) => chatSendKey.set(next ? "mod-enter" : "enter")}
      data-testid={CHAT_SHORTCUT_ROWS[0]}
    />
    <SettingsToggle
      id={`${CHAT_SHORTCUT_ROWS[1]}-switch`}
      label={$t(regenerateKeys.labelKey)}
      description={$t(regenerateKeys.hintKey)}
      checked={regenerate.value}
      source={regenerate.source}
      onChange={(next) => void writeBehaviour(CHAT_BEHAVIOUR_KEYS.regenerateShortcut, next)}
      data-testid={CHAT_SHORTCUT_ROWS[1]}
    />
  </SettingsSection>

  <SettingsSection title={$t(CHAT_SETTINGS_BLOCKS[2])}>
    {#snippet icon()}<Tag size={15} strokeWidth={1.75} />{/snippet}
    <div class="flex flex-col gap-3" data-testid="chat-auto-naming">
      <p class="text-sm font-medium text-foreground">
        {$t("settings.chat.naming_label")}
      </p>
      <RadioGroup
        value={$chatAutoNaming}
        data-testid="chat-auto-naming-group"
      >
        {#each CHAT_NAMING_CHOICES as choice (choice.value)}
          <div class="flex flex-col gap-0.5">
            <RadioItem
              value={choice.value}
              checked={$chatAutoNaming === choice.value}
              disabled={locked}
              onchange={chooseNaming}
              data-testid="chat-auto-naming-{choice.value}"
            >
              {$t(choice.labelKey)}
            </RadioItem>
            <p class="pl-6 text-xs leading-relaxed text-muted-foreground">
              {$t(choice.hintKey)}
            </p>
          </div>
        {/each}
      </RadioGroup>
      {#if locked}
        <p
          class="inline-flex items-center gap-1.5 text-xs text-muted-foreground"
          title={$t("settings.policy_locked_hint")}
          data-testid="chat-auto-naming-policy"
        >
          <Lock class="h-3 w-3" aria-hidden="true" />
          {$t("settings.policy_locked")}
        </p>
      {/if}
    </div>
  </SettingsSection>

  <h2 class="px-1 pt-2 text-overline uppercase text-muted-foreground">
    {$t(CHAT_SETTINGS_BLOCKS[3])}
  </h2>
  <ShortcutCatalog />
</SettingsSubPage>
