<script module lang="ts">
  import type { ChatAutoNaming } from "$lib/stores/chatAutoNaming";
  import type { SettingSource } from "$lib/ipc/settings";

  /** The blocks of the section, in order, by the i18n key of their title. */
  export const CHAT_SETTINGS_BLOCKS = [
    "settings.chat.conversations_title",
    "settings.chat.shortcuts_title",
    "settings.chat.naming_title",
    "settings.chat.all_shortcuts_title",
  ] as const;

  /** The rows of the keyboard shortcuts block, by `data-testid`. */
  export const CHAT_SHORTCUT_ROWS = ["chat-send-key"] as const;

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
  const locked = $derived(namingLocked($chatAutoNamingSource));

  onMount(() => {
    void hydrateChatAutoNaming();
  });

  function chooseNaming(value: string): void {
    if (locked || !isChatAutoNaming(value)) return;
    void setChatAutoNaming(value);
  }
</script>

<SettingsSubPage route="chat" data-testid="chat-settings-section">
  <SettingsSection title={$t(CHAT_SETTINGS_BLOCKS[0])} bodyLayout="flush">
    {#snippet icon()}<MessageSquare size={15} strokeWidth={1.75} />{/snippet}
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
