<script lang="ts">
  import { t } from "svelte-i18n";
  import { Plus, LayoutList, AlignJustify, Rows3, FolderOpen } from "lucide-svelte";
  import ChatSessionCard from "./ChatSessionCard.svelte";
  import SessionSearchField from "./SessionSearchField.svelte";
  import SessionFilters from "./SessionFilters.svelte";
  import {
    sessionFilter,
    sidebarDensity,
    pinnedVisibleSessions,
    unpinnedVisibleSessions,
    visibleSessions,
    sessionProjectFilter,
    togglePinned,
    toggleArchived,
    type SidebarDensity,
  } from "$lib/stores/chatSessions";
  import { projects } from "$lib/stores/projects";

  interface Props {
    selectedSessionId: string | null;
    onNewChat: () => void;
    onSelect: (id: string) => void;
    onDelete: (id: string) => void;
    onRename: (id: string, title: string) => void;
  }

  let { selectedSessionId, onNewChat, onSelect, onDelete, onRename }: Props = $props();

  const DENSITIES: Array<{ id: SidebarDensity; icon: typeof AlignJustify; labelKey: string }> = [
    { id: "sm", icon: AlignJustify, labelKey: "chat.density_compact" },
    { id: "md", icon: LayoutList, labelKey: "chat.density_default" },
    { id: "lg", icon: Rows3, labelKey: "chat.density_comfortable" },
  ];

  const emptyKey = $derived.by(() => {
    switch ($sessionFilter) {
      case "pinned":
        return "chat.empty_pinned";
      case "archived":
        return "chat.empty_archived";
      case "closed":
        return "chat.empty_closed";
      default:
        return "chat.empty_active";
    }
  });
</script>

<div class="flex h-full min-h-0 flex-col" data-testid="chat-sessions-sidebar">
  <!-- Sticky header: new chat CTA, project selector, search, filters. -->
  <div
    class="sticky top-0 z-10 shrink-0 space-y-1.5 bg-background/85 backdrop-blur
      px-2 pt-2 pb-2 border-b border-border/20"
  >
    <button
      class="flex items-center justify-center gap-1.5 rounded-lg px-3 py-2 text-[11px] font-medium
        bg-primary/10 text-primary ring-1 ring-primary/20
        hover:bg-primary/15 hover:ring-primary/30 transition-all active:scale-[0.98] w-full"
      onclick={onNewChat}
      data-testid="sidebar-new-chat"
    >
      <Plus size={12} />
      {$t("chat.new_chat")}
    </button>

    {#if $projects.length > 0}
      <label class="flex items-center gap-1 text-[10px] text-muted-foreground/70">
        <FolderOpen size={10} class="shrink-0" />
        <select
          class="h-6 flex-1 min-w-0 rounded border border-border/40 bg-background/60 px-1 text-[10px]
            focus:outline-none focus:ring-1 focus:ring-primary/40"
          value={$sessionProjectFilter ?? ""}
          onchange={(e) => sessionProjectFilter.set(e.currentTarget.value || null)}
          data-testid="sidebar-project-filter"
        >
          <option value="">{$t("chat.all_projects")}</option>
          {#each $projects as proj (proj.id)}
            <option value={proj.id}>{proj.name}</option>
          {/each}
        </select>
      </label>
    {/if}

    <SessionSearchField />
    <SessionFilters />

    <!-- Density toggle -->
    <div class="flex items-center justify-end gap-0.5 pt-0.5">
      {#each DENSITIES as d (d.id)}
        {@const active = $sidebarDensity === d.id}
        {@const Icon = d.icon}
        <button
          type="button"
          class="rounded p-1 text-[10px] transition-colors
            {active ? 'bg-muted/60 text-foreground' : 'text-muted-foreground/50 hover:bg-muted/40 hover:text-foreground'}"
          title={$t(d.labelKey)}
          aria-label={$t(d.labelKey)}
          onclick={() => sidebarDensity.set(d.id)}
          data-testid="sidebar-density-{d.id}"
        >
          <Icon size={11} />
        </button>
      {/each}
    </div>
  </div>

  <!-- Scrollable list -->
  <div class="flex-1 min-h-0 overflow-y-auto px-2 pb-2" data-testid="chat-sessions-list">
    {#if $visibleSessions.length === 0}
      <p class="px-3 pt-8 pb-4 text-[11px] text-muted-foreground/50 text-center italic">
        {$t(emptyKey)}
      </p>
    {:else}
      {#if $pinnedVisibleSessions.length > 0}
        <p
          class="px-3 pt-2 pb-1.5 text-[10px] font-semibold uppercase tracking-wider text-amber-600/70"
          data-testid="sidebar-pinned-section"
        >
          {$t("chat.pinned_sessions")}
        </p>
        <div class="space-y-1">
          {#each $pinnedVisibleSessions as session (session.id)}
            <ChatSessionCard
              {session}
              selected={selectedSessionId === session.id}
              density={$sidebarDensity}
              onclick={onSelect}
              ondelete={onDelete}
              onrename={onRename}
              onpin={togglePinned}
              onarchive={toggleArchived}
            />
          {/each}
        </div>
      {/if}

      {#if $unpinnedVisibleSessions.length > 0}
        {#if $pinnedVisibleSessions.length > 0}
          <p class="px-3 pt-3 pb-1.5 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground/50">
            {$t(`chat.filter_${$sessionFilter}`)}
          </p>
        {/if}
        <div class="space-y-1">
          {#each $unpinnedVisibleSessions as session (session.id)}
            <ChatSessionCard
              {session}
              selected={selectedSessionId === session.id}
              density={$sidebarDensity}
              onclick={onSelect}
              ondelete={onDelete}
              onrename={onRename}
              onpin={togglePinned}
              onarchive={toggleArchived}
            />
          {/each}
        </div>
      {/if}
    {/if}
  </div>
</div>
