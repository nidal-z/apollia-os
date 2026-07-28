<script lang="ts" module>
  export type ProjectTab =
    | "conversations"
    | "tasks"
    | "memory"
    | "context"
    | "settings";
</script>

<script lang="ts">
  /**
   * ProjectDetailPane - right pane of the Projects split.
   *
   * Composes the operator `DetailHeader` + `TabBar` and routes to the four
   * project tabs. Selection changes remount this pane (the parent keys it by
   * project id) which is what drives the master -> detail `routeTransition`.
   * All IPC lives either in the parent or inside `ProjectSettingsTab`; this
   * pane only forwards typed data and callbacks.
   */
  import { t } from "svelte-i18n";
  import { Folder, Sparkles, Settings, MessageCircle } from "lucide-svelte";
  import { DetailHeader, StatusDot, type Task as RowTask } from "$lib/components/operator";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { TabBar } from "$lib/components/ui/tabs";
  import ContextProvidersTab from "../project/ContextProvidersTab.svelte";
  import ProjectConversationsTab from "./ProjectConversationsTab.svelte";
  import ProjectTasksTab from "./ProjectTasksTab.svelte";
  import ProjectMemoryTab from "./ProjectMemoryTab.svelte";
  import ProjectSettingsTab from "./ProjectSettingsTab.svelte";
  import type { ChatSessionSummary, ProjectDetail } from "$lib/types";

  interface MemoryNamespace {
    namespace: string;
    subname: string;
    count: number;
  }

  interface Props {
    project: ProjectDetail;
    activeTab: ProjectTab;
    onTabChange: (tab: ProjectTab) => void;
    onNewChat: () => void;
    onContextUpdated: () => void;
    onProjectSaved: (updated: ProjectDetail) => void;
    onRequestDelete: (id: string, name: string) => void;
    chats: ChatSessionSummary[];
    chatsLoading: boolean;
    onOpenChat: (sessionId: string) => void;
    formatRelative: (iso: string | null | undefined) => string;
    taskRows: RowTask[];
    tasksLoading: boolean;
    namespaces: MemoryNamespace[];
    memoryLoading: boolean;
  }

  let {
    project,
    activeTab,
    onTabChange,
    onNewChat,
    onContextUpdated,
    onProjectSaved,
    onRequestDelete,
    chats,
    chatsLoading,
    onOpenChat,
    formatRelative,
    taskRows,
    tasksLoading,
    namespaces,
    memoryLoading,
  }: Props = $props();

  const agentCount = $derived(project.agents.length);
  const tabItems = $derived([
    { key: "conversations", label: $t("projects.tab_conversations") },
    { key: "tasks", label: $t("projects.tab_tasks") },
    { key: "memory", label: $t("projects.tab_memory") },
    { key: "context", label: $t("projects.tab_context") },
    { key: "settings", label: $t("projects.tab_settings") },
  ]);
</script>

<DetailHeader title={project.name} meta={project.description || undefined}>
  {#snippet leading()}
    <div
      class="w-10 h-10 shrink-0 rounded-lg inline-flex items-center justify-center signature-gradient-border"
    >
      <Folder size={16} class="text-primary" />
    </div>
  {/snippet}
  {#snippet badges()}
    <Badge variant="primary" size="sm">
      {#snippet icon()}<StatusDot color="hsl(var(--primary))" glow />{/snippet}
      {$t("projects.status_active")}
    </Badge>
    {#if agentCount > 0}
      <span class="inline-flex items-center gap-1.5">
        <span class="flex">
          {#each project.agents.slice(0, 3) as _, i (i)}
            <span
              class="w-5 h-5 rounded-full inline-flex items-center justify-center border-2 border-background {i === 0 ? '' : '-ml-2'}"
              style="background: linear-gradient(135deg, hsl(var(--primary)), hsl(var(--secondary)));"
            >
              <Sparkles size={9} class="text-primary-foreground" />
            </span>
          {/each}
        </span>
        <span class="text-caption text-muted-foreground tabular-nums">
          {$t("projects.section_agents")}: {agentCount}
        </span>
      </span>
    {/if}
  {/snippet}
  {#snippet actions()}
    <Button variant="outline" size="sm" onclick={() => onTabChange("settings")}>
      {#snippet icon()}<Settings size={12} />{/snippet}
      {$t("projects.tab_settings")}
    </Button>
    <Button variant="primary-solid" size="sm" onclick={onNewChat}>
      {#snippet icon()}<MessageCircle size={12} />{/snippet}
      {$t("projects.new_chat")}
    </Button>
  {/snippet}
</DetailHeader>

<div class="px-8 pt-3.5">
  <TabBar
    variant="underline"
    testidPrefix="project"
    {activeTab}
    items={tabItems}
    ontabchange={(key) => onTabChange(key as ProjectTab)}
  />
</div>

<div class="flex-1 overflow-auto">
  {#if activeTab === "conversations"}
    <ProjectConversationsTab {chats} loading={chatsLoading} {formatRelative} onOpen={onOpenChat} />
  {:else if activeTab === "tasks"}
    <ProjectTasksTab rows={taskRows} loading={tasksLoading} hasAgents={agentCount > 0} />
  {:else if activeTab === "memory"}
    <ProjectMemoryTab documents={project.documents} {namespaces} loading={memoryLoading} />
  {:else if activeTab === "context"}
    <ContextProvidersTab {project} onUpdated={onContextUpdated} />
  {:else}
    <ProjectSettingsTab {project} onSaved={onProjectSaved} onRequestDelete={onRequestDelete} />
  {/if}
</div>
