<script lang="ts" context="module">
  export const meta = {
    title: "settings.nav.help",
    icon: "life-buoy",
    group: "settings.nav.cluster_help",
    cluster: "help",
  } as const;
</script>

<script lang="ts">
  import { t } from "svelte-i18n";
  import {
    Cpu,
    MessageSquare,
    Mic,
    ListChecks,
    UserRound,
    BookOpen,
    ChevronRight,
    ExternalLink,
  } from "lucide-svelte";
  import { Card } from "$lib/components/ui/card";
  import { navigateTo } from "$lib/stores/navigation";
  import { navigateToSettings } from "$lib/router";
  import { handleExternalLinkClick } from "$lib/utils/externalLink";

  const DOCS_URL = "https://github.com/Apollia-OS/apollia-os/wiki";

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
      run: () => navigateToSettings("model-hub"),
    },
    {
      id: "chat",
      icon: MessageSquare,
      titleKey: "settings.help.topic_chat_title",
      bodyKey: "settings.help.topic_chat_body",
      actionKey: "settings.help.topic_chat_action",
      run: () => navigateTo("chat"),
    },
    {
      id: "stt",
      icon: Mic,
      titleKey: "settings.help.topic_stt_title",
      bodyKey: "settings.help.topic_stt_body",
      actionKey: "settings.help.topic_stt_action",
      run: () => navigateToSettings("stt"),
    },
    {
      id: "plan",
      icon: ListChecks,
      titleKey: "settings.help.topic_plan_title",
      bodyKey: "settings.help.topic_plan_body",
      actionKey: "settings.help.topic_plan_action",
      run: () => navigateTo("chat"),
    },
    {
      id: "profile",
      icon: UserRound,
      titleKey: "settings.help.topic_profile_title",
      bodyKey: "settings.help.topic_profile_body",
      actionKey: "settings.help.topic_profile_action",
      run: () => navigateToSettings("profile"),
    },
  ];
</script>

<section class="mx-auto max-w-[720px] space-y-5" data-testid="help-section">
  <!-- Intro -->
  <Card class="rounded-xl p-5" data-testid="help-intro">
    <div class="flex items-start gap-3">
      <BookOpen size={18} strokeWidth={1.75} class="mt-0.5 shrink-0 text-muted-foreground" aria-hidden="true" />
      <p class="text-[13px] leading-[1.65] text-muted-foreground">
        {$t("settings.help.intro")}
      </p>
    </div>
  </Card>

  <!-- Getting-started topics -->
  <div class="space-y-2.5">
    <h2 class="px-1 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
      {$t("settings.help.getting_started")}
    </h2>
    {#each topics as topic (topic.id)}
      {@const TopicIcon = topic.icon}
      <button
        type="button"
        onclick={topic.run}
        class="group flex w-full items-center gap-3 rounded-xl border border-border bg-card p-4 text-left transition-colors hover:border-primary/40 hover:bg-muted/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40"
        data-testid="help-topic-{topic.id}"
      >
        <span class="inline-flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-muted text-primary">
          <TopicIcon size={18} strokeWidth={1.75} aria-hidden="true" />
        </span>
        <span class="min-w-0 flex-1">
          <span class="block text-[13px] font-medium text-foreground">{$t(topic.titleKey)}</span>
          <span class="mt-0.5 block text-[11.5px] leading-relaxed text-muted-foreground">
            {$t(topic.bodyKey)}
          </span>
        </span>
        <span class="inline-flex shrink-0 items-center gap-1 text-[11.5px] font-medium text-muted-foreground transition-colors group-hover:text-primary">
          <span class="hidden sm:inline">{$t(topic.actionKey)}</span>
          <ChevronRight size={14} strokeWidth={2} aria-hidden="true" />
        </span>
      </button>
    {/each}
  </div>

  <!-- Full docs -->
  <a
    href={DOCS_URL}
    rel="noreferrer"
    onclick={handleExternalLinkClick}
    class="group flex items-center gap-3 rounded-xl border border-border bg-muted/30 p-4 transition-colors hover:border-primary/40 hover:bg-muted/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40"
    data-testid="help-link-docs"
  >
    <span class="inline-flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-card text-primary">
      <BookOpen size={18} strokeWidth={1.75} aria-hidden="true" />
    </span>
    <span class="min-w-0 flex-1">
      <span class="flex items-center gap-1.5 text-[13px] font-medium text-foreground">
        {$t("settings.help.docs_title")}
        <ExternalLink size={12} class="text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100" aria-hidden="true" />
      </span>
      <span class="block text-[11.5px] text-muted-foreground">{$t("settings.help.docs_desc")}</span>
    </span>
  </a>
</section>
