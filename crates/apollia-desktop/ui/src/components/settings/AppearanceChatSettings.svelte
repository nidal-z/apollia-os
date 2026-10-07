<!--
  Settings > Appearance, the two blocks about reading the chat: "Chat style"
  (rendered or plain text, text size and weight, generation figures, scroll on
  send and while an answer arrives, bubbles or blocks, density, width) and
  "Reasoning" (unfold by default, vignette).
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
    CHAT_MESSAGE_STYLES,
    CHAT_VIEW_MODES,
    GENERATION_INFO_SCOPES,
    type ChatDensity,
    type ChatMessageStyle,
    type ChatViewMode,
    type GenerationInfoScope,
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

  const VIEW_MODE_LABEL_KEYS: Record<ChatViewMode, string> = {
    markdown: "settings.appearance.view_mode_markdown",
    plain: "settings.appearance.view_mode_plain",
  };
  const GENERATION_INFO_LABEL_KEYS: Record<GenerationInfoScope, string> = {
    last: "settings.appearance.generation_info_last",
    all: "settings.appearance.generation_info_all",
  };
  const MESSAGE_STYLE_LABEL_KEYS: Record<ChatMessageStyle, string> = {
    bubble: "settings.appearance.message_style_bubble",
    block: "settings.appearance.message_style_block",
  };

  function onMessageStyleKeydown(e: KeyboardEvent): void {
    const dir =
      e.key === "ArrowRight" || e.key === "ArrowDown"
        ? 1
        : e.key === "ArrowLeft" || e.key === "ArrowUp"
          ? -1
          : 0;
    if (dir === 0) return;
    e.preventDefault();
    const n = CHAT_MESSAGE_STYLES.length;
    const next =
      CHAT_MESSAGE_STYLES[(CHAT_MESSAGE_STYLES.indexOf($chatAppearance.messageStyle) + dir + n) % n];
    setChatAppearance({ messageStyle: next });
    (e.currentTarget as HTMLElement).querySelector<HTMLElement>(`[data-value="${next}"]`)?.focus();
  }

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
    label={$t("settings.appearance.view_mode_label")}
    hint={$t("settings.appearance.view_mode_hint")}
    for="appearance-view-mode"
    data-testid="appearance-view-mode"
  >
    {#snippet control()}
      <Select
        id="appearance-view-mode"
        size="sm"
        value={$chatAppearance.viewMode}
        onchange={(e) => setChatAppearance({ viewMode: e.currentTarget.value as ChatViewMode })}
        data-testid="appearance-view-mode-select"
      >
        {#each CHAT_VIEW_MODES as mode (mode)}
          <option value={mode}>{$t(VIEW_MODE_LABEL_KEYS[mode])}</option>
        {/each}
      </Select>
    {/snippet}
  </SettingsFieldRow>

  <SettingsFieldRow
    label={$t("settings.appearance.tab_scrollbar_label")}
    hint={$t("settings.appearance.tab_scrollbar_hint")}
    for="appearance-tab-scrollbar"
    data-testid="appearance-tab-scrollbar"
  >
    {#snippet control()}
      <Toggle
        id="appearance-tab-scrollbar"
        checked={$chatAppearance.tabScrollbar}
        onchange={(checked) => setChatAppearance({ tabScrollbar: checked })}
        aria-label={$t("settings.appearance.tab_scrollbar_label")}
        data-testid="appearance-tab-scrollbar-toggle"
      />
    {/snippet}
  </SettingsFieldRow>

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
    label={$t("settings.appearance.generation_info_label")}
    help={$t("settings.appearance.generation_info_help")}
    for="appearance-generation-info"
    data-testid="appearance-generation-info"
  >
    {#snippet control()}
      <Select
        id="appearance-generation-info"
        size="sm"
        value={$chatAppearance.generationInfo}
        onchange={(e) =>
          setChatAppearance({ generationInfo: e.currentTarget.value as GenerationInfoScope })}
        data-testid="appearance-generation-info-select"
      >
        {#each GENERATION_INFO_SCOPES as scope (scope)}
          <option value={scope}>{$t(GENERATION_INFO_LABEL_KEYS[scope])}</option>
        {/each}
      </Select>
    {/snippet}
  </SettingsFieldRow>

  <SettingsFieldRow
    label={$t("settings.appearance.scroll_sent_top_label")}
    for="appearance-scroll-sent-top"
    border={false}
    data-testid="appearance-scroll-sent-top"
  >
    {#snippet control()}
      <Toggle
        id="appearance-scroll-sent-top"
        checked={$chatAppearance.scrollSentToTop}
        onchange={(checked) => setChatAppearance({ scrollSentToTop: checked })}
        aria-label={$t("settings.appearance.scroll_sent_top_label")}
        data-testid="appearance-scroll-sent-top-toggle"
      />
    {/snippet}
  </SettingsFieldRow>

  <SettingsFieldRow
    label={$t("settings.appearance.follow_generation_label")}
    for="appearance-follow-generation"
    indent
    data-testid="appearance-follow-generation"
  >
    {#snippet control()}
      <Toggle
        id="appearance-follow-generation"
        checked={$chatAppearance.followGeneration}
        onchange={(checked) => setChatAppearance({ followGeneration: checked })}
        aria-label={$t("settings.appearance.follow_generation_label")}
        data-testid="appearance-follow-generation-toggle"
      />
    {/snippet}
  </SettingsFieldRow>

  <SettingsFieldRow
    label={$t("settings.appearance.message_style_label")}
    data-testid="appearance-message-style"
  >
    {#snippet control()}
      <div
        role="radiogroup"
        tabindex={-1}
        aria-label={$t("settings.appearance.message_style_label")}
        class="inline-flex gap-0.5 rounded-lg border border-border bg-muted/60 p-0.5"
        onkeydown={onMessageStyleKeydown}
        data-testid="appearance-message-style-toggle"
      >
        {#each CHAT_MESSAGE_STYLES as style (style)}
          {@const active = $chatAppearance.messageStyle === style}
          <button
            type="button"
            role="radio"
            aria-checked={active}
            tabindex={active ? 0 : -1}
            data-value={style}
            class="rounded-md px-3 py-1.5 text-label-sm ring-offset-background transition-all duration-base focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-offset-1
              {active
              ? 'bg-card text-foreground shadow-sm'
              : 'text-muted-foreground hover:text-foreground'}"
            onclick={() => setChatAppearance({ messageStyle: style })}
            data-testid="appearance-message-style-{style}"
          >
            {$t(MESSAGE_STYLE_LABEL_KEYS[style])}
          </button>
        {/each}
      </div>
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

  <SettingsFieldRow
    label={$t("settings.appearance.reasoning_vignette_label")}
    hint={$t("settings.appearance.reasoning_vignette_hint")}
    for="appearance-reasoning-vignette"
    border={false}
    data-testid="appearance-reasoning-vignette"
  >
    {#snippet control()}
      <Toggle
        id="appearance-reasoning-vignette"
        checked={$chatAppearance.reasoningVignette}
        onchange={(checked) => setChatAppearance({ reasoningVignette: checked })}
        aria-label={$t("settings.appearance.reasoning_vignette_label")}
        data-testid="appearance-reasoning-vignette-toggle"
      />
    {/snippet}
  </SettingsFieldRow>
</SettingsSection>
