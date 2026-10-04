<!--
  Settings > Appearance, the two blocks about reading the chat: "Chat style"
  (text size and weight, density, width) and "Reasoning" (unfold by default).
  The values live in `lib/stores/chatAppearance.ts`; this file only edits them.
-->
<script lang="ts">
  import { t } from "svelte-i18n";
  import { Type, Brain } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Select } from "$lib/components/ui/select";
  import { Toggle } from "$lib/components/ui/toggle";
  import {
    chatAppearance,
    setChatAppearance,
    resetChatFontSize,
    CHAT_APPEARANCE_DEFAULTS,
    CHAT_DENSITIES,
    CHAT_FONT_SIZE_MAX,
    CHAT_FONT_SIZE_MIN,
    CHAT_FONT_WEIGHTS,
    type ChatDensity,
    type ChatFontSize,
    type ChatFontWeight,
  } from "$lib/stores/chatAppearance";
  import SettingsSection from "./SettingsSection.svelte";
  import SettingsFieldRow from "./SettingsFieldRow.svelte";

  const WEIGHT_LABEL_KEYS: Record<ChatFontWeight, string> = {
    normal: "settings.appearance.font_weight_normal",
    medium: "settings.appearance.font_weight_medium",
  };
  const DENSITY_LABEL_KEYS: Record<ChatDensity, string> = {
    comfortable: "settings.appearance.density_comfortable",
    compact: "settings.appearance.density_compact",
  };

  const atDefaultSize = $derived(
    $chatAppearance.fontSize === CHAT_APPEARANCE_DEFAULTS.fontSize,
  );

  function onFontSizeInput(e: Event & { currentTarget: HTMLInputElement }): void {
    const step = Number(e.currentTarget.value);
    if (Number.isInteger(step) && step >= CHAT_FONT_SIZE_MIN && step <= CHAT_FONT_SIZE_MAX) {
      setChatAppearance({ fontSize: step as ChatFontSize });
    }
  }
</script>

<SettingsSection title={$t("settings.appearance.chat_style_title")} bodyLayout="flush">
  {#snippet icon()}<Type size={15} strokeWidth={1.75} />{/snippet}

  <SettingsFieldRow
    label={$t("settings.appearance.font_size_label")}
    hint={$t("settings.appearance.font_size_hint")}
    for="appearance-font-size"
    data-testid="appearance-font-size"
  >
    {#snippet control()}
      <div class="flex items-center gap-3">
        <input
          id="appearance-font-size"
          type="range"
          min={CHAT_FONT_SIZE_MIN}
          max={CHAT_FONT_SIZE_MAX}
          step="1"
          value={$chatAppearance.fontSize}
          oninput={onFontSizeInput}
          aria-valuetext={$t("settings.appearance.font_size_value", {
            values: { step: $chatAppearance.fontSize, max: CHAT_FONT_SIZE_MAX },
          })}
          class="h-1 w-40 cursor-pointer accent-primary"
          data-testid="appearance-font-size-slider"
        />
        <Button
          variant="ghost"
          size="sm"
          disabled={atDefaultSize}
          onclick={resetChatFontSize}
          data-testid="appearance-font-size-reset"
        >
          {$t("settings.appearance.font_size_reset")}
        </Button>
      </div>
    {/snippet}
  </SettingsFieldRow>

  <SettingsFieldRow
    label={$t("settings.appearance.font_weight_label")}
    for="appearance-font-weight"
    data-testid="appearance-font-weight"
  >
    {#snippet control()}
      <Select
        id="appearance-font-weight"
        size="sm"
        value={$chatAppearance.fontWeight}
        onchange={(e) =>
          setChatAppearance({ fontWeight: e.currentTarget.value as ChatFontWeight })}
        data-testid="appearance-font-weight-select"
      >
        {#each CHAT_FONT_WEIGHTS as weight (weight)}
          <option value={weight}>{$t(WEIGHT_LABEL_KEYS[weight])}</option>
        {/each}
      </Select>
    {/snippet}
  </SettingsFieldRow>

  <SettingsFieldRow
    label={$t("settings.appearance.density_label")}
    hint={$t("settings.appearance.density_hint")}
    for="appearance-density"
    data-testid="appearance-density"
  >
    {#snippet control()}
      <Select
        id="appearance-density"
        size="sm"
        value={$chatAppearance.density}
        onchange={(e) => setChatAppearance({ density: e.currentTarget.value as ChatDensity })}
        data-testid="appearance-density-select"
      >
        {#each CHAT_DENSITIES as density (density)}
          <option value={density}>{$t(DENSITY_LABEL_KEYS[density])}</option>
        {/each}
      </Select>
    {/snippet}
  </SettingsFieldRow>

  <SettingsFieldRow
    label={$t("settings.appearance.full_width_label")}
    for="appearance-full-width"
    border={false}
    data-testid="appearance-full-width"
  >
    {#snippet control()}
      <Toggle
        id="appearance-full-width"
        checked={$chatAppearance.fullWidth}
        onchange={(checked) => setChatAppearance({ fullWidth: checked })}
        aria-label={$t("settings.appearance.full_width_label")}
        data-testid="appearance-full-width-toggle"
      />
    {/snippet}
  </SettingsFieldRow>
</SettingsSection>

<SettingsSection title={$t("settings.appearance.reasoning_title")} bodyLayout="flush">
  {#snippet icon()}<Brain size={15} strokeWidth={1.75} />{/snippet}

  <SettingsFieldRow
    label={$t("settings.appearance.reasoning_expand_label")}
    hint={$t("settings.appearance.reasoning_expand_hint")}
    for="appearance-reasoning-expand"
    border={false}
    data-testid="appearance-reasoning-expand"
  >
    {#snippet control()}
      <Toggle
        id="appearance-reasoning-expand"
        checked={$chatAppearance.expandReasoning}
        onchange={(checked) => setChatAppearance({ expandReasoning: checked })}
        aria-label={$t("settings.appearance.reasoning_expand_label")}
        data-testid="appearance-reasoning-expand-toggle"
      />
    {/snippet}
  </SettingsFieldRow>
</SettingsSection>
