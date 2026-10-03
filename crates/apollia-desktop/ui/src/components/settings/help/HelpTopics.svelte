<script module lang="ts">
  import { navigateTo, type Route } from "$lib/stores/navigation";
  import { navigateToSettings } from "$lib/router";
  import type { SettingsSubRoute } from "$lib/stores/settings";

  /** Where a topic leads: a screen of the application, or a settings section. */
  export type HelpTopicTarget = { route: Route } | { settings: SettingsSubRoute };

  /**
   * The model topic leads to the AI screen, where models are chosen and run;
   * dictation and profile stay settings sections.
   */
  export const HELP_TOPIC_TARGETS: Readonly<Record<string, HelpTopicTarget>> = {
    model: { route: "llm" },
    chat: { route: "chat" },
    stt: { settings: "stt" },
    plan: { route: "chat" },
    profile: { settings: "profile" },
  };

  /** Follow a topic: a screen closes the settings window, a section shows in it. */
  export function openHelpTopic(id: string): void {
    const target = HELP_TOPIC_TARGETS[id];
    if (!target) return;
    if ("route" in target) navigateTo(target.route);
    else navigateToSettings(target.settings);
  }
</script>

<script lang="ts">
  /**
   * HelpTopics - the "getting started" list of in-app shortcuts.
   *
   * Each row navigates inside Apollia (never a browser), so it is a real
   * `<button>` with a trailing chevron, not an external link.
   */
  import { t } from "svelte-i18n";
  import {
    Cpu,
    MessageSquare,
    Mic,
    ListChecks,
    UserRound,
    ChevronRight,
  } from "lucide-svelte";

  type Icon = typeof Cpu;

  type HelpTopic = {
    id: string;
    icon: Icon;
    titleKey: string;
    bodyKey: string;
    actionKey: string;
    run: () => void;
  };

  const topics: HelpTopic[] = [
    {
      id: "model",
      icon: Cpu,
      titleKey: "settings.help.topic_model_title",
      bodyKey: "settings.help.topic_model_body",
      actionKey: "settings.help.topic_model_action",
      run: () => openHelpTopic("model"),
    },
    {
      id: "chat",
      icon: MessageSquare,
      titleKey: "settings.help.topic_chat_title",
      bodyKey: "settings.help.topic_chat_body",
      actionKey: "settings.help.topic_chat_action",
      run: () => openHelpTopic("chat"),
    },
    {
      id: "stt",
      icon: Mic,
      titleKey: "settings.help.topic_stt_title",
      bodyKey: "settings.help.topic_stt_body",
      actionKey: "settings.help.topic_stt_action",
      run: () => openHelpTopic("stt"),
    },
    {
      id: "plan",
      icon: ListChecks,
      titleKey: "settings.help.topic_plan_title",
      bodyKey: "settings.help.topic_plan_body",
      actionKey: "settings.help.topic_plan_action",
      run: () => openHelpTopic("plan"),
    },
    {
      id: "profile",
      icon: UserRound,
      titleKey: "settings.help.topic_profile_title",
      bodyKey: "settings.help.topic_profile_body",
      actionKey: "settings.help.topic_profile_action",
      run: () => openHelpTopic("profile"),
    },
  ];
</script>

<section class="space-y-2.5" aria-label={$t("settings.help.getting_started")}>
  <h2 class="px-1 text-overline uppercase text-muted-foreground">
    {$t("settings.help.getting_started")}
  </h2>
  {#each topics as topic (topic.id)}
    <button
      type="button"
      onclick={topic.run}
      class="group flex w-full items-center gap-3 rounded-xl border border-border bg-card p-4 text-left
        transition hover:translate-x-0.5 hover:border-primary/40 hover:bg-muted/40 hover:shadow-sm
        focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40
        motion-reduce:transition-none motion-reduce:hover:translate-x-0"
      data-testid="help-topic-{topic.id}"
    >
      <span class="grid h-9 w-9 shrink-0 place-items-center rounded-lg bg-muted text-primary">
        <topic.icon size={18} strokeWidth={1.75} aria-hidden="true" />
      </span>
      <span class="min-w-0 flex-1">
        <span class="block text-body-sm font-medium text-foreground">{$t(topic.titleKey)}</span>
        <span class="mt-0.5 block text-body-xs leading-relaxed text-muted-foreground">
          {$t(topic.bodyKey)}
        </span>
      </span>
      <span
        class="inline-flex shrink-0 items-center gap-1 text-body-xs font-medium text-muted-foreground
          transition-colors group-hover:text-primary"
      >
        <span class="hidden sm:inline">{$t(topic.actionKey)}</span>
        <ChevronRight size={14} strokeWidth={2} aria-hidden="true" />
      </span>
    </button>
  {/each}
</section>
