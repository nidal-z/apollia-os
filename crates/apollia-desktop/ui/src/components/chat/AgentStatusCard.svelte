<script lang="ts">
  /**
   * Single agent card with avatar, skill chips and a live status dot
   * Rendered inside the QuickPicker agents grid.
   */
  import { t } from "svelte-i18n";
  import { Bot } from "lucide-svelte";
  import type { AgentListItem } from "$lib/types";
  import type { AgentLiveStatus } from "$lib/stores/agentStatus";

  interface Props {
    agent: AgentListItem;
    status: AgentLiveStatus;
    disabled?: boolean;
    onselect: (name: string) => void;
  }

  let { agent, status, disabled = false, onselect }: Props = $props();

  const DOT_CLASS: Record<AgentLiveStatus, string> = {
    online: "bg-emerald-500",
    busy: "bg-amber-500 animate-pulse",
    offline: "bg-muted-foreground/40",
    error: "bg-red-500",
  };

  const skills = $derived(agent.skills ?? []);
  const topSkills = $derived(skills.slice(0, 3));
  const hiddenSkillCount = $derived(Math.max(0, skills.length - topSkills.length));
</script>

<button
  type="button"
  class="group flex w-full flex-col gap-2 rounded-lg border border-border/40 bg-card/60 px-3 py-2.5 text-left
    transition-all hover:border-primary/40 hover:bg-primary/5 active:scale-[0.99]
    focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40 disabled:opacity-50 disabled:pointer-events-none"
  onclick={() => onselect(agent.name)}
  {disabled}
  data-testid="quickpicker-agent-{agent.name}"
>
  <div class="flex items-center gap-2">
    <div
      class="flex h-7 w-7 shrink-0 items-center justify-center rounded-md bg-muted/60 text-muted-foreground"
      aria-hidden="true"
    >
      <Bot size={14} />
    </div>
    <span class="flex-1 truncate text-[12px] font-medium text-foreground">{agent.name}</span>
    <span
      class="relative inline-flex h-2 w-2 rounded-full {DOT_CLASS[status]}"
      title={$t(`chat.agent_status.${status}`)}
      aria-label={$t(`chat.agent_status.${status}`)}
    ></span>
  </div>

  {#if agent.description}
    <p class="line-clamp-2 text-[11px] text-muted-foreground/80">{agent.description}</p>
  {/if}

  {#if topSkills.length > 0}
    <div class="flex flex-wrap gap-1">
      {#each topSkills as skill (skill.id)}
        <span
          class="rounded bg-secondary/10 px-1.5 py-0.5 text-[10px] text-secondary/80"
          title={skill.description}
        >
          {skill.name}
        </span>
      {/each}
      {#if hiddenSkillCount > 0}
        <span class="rounded px-1.5 py-0.5 text-[10px] text-muted-foreground/50">
          +{hiddenSkillCount}
        </span>
      {/if}
    </div>
  {/if}
</button>
