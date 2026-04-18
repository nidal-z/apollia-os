<script lang="ts">
  import { t } from "svelte-i18n";
  import { User, Settings, Wrench, Building2, Bot } from "lucide-svelte";
  import { Check } from "lucide-svelte";

  interface Props {
    topicsCovered: string[];
    activeTopic?: string | null;
  }

  let { topicsCovered, activeTopic = null }: Props = $props();

  const TOPICS = [
    { id: "identity", icon: User },
    { id: "preferences", icon: Settings },
    { id: "tools", icon: Wrench },
    { id: "domain", icon: Building2 },
    { id: "agents", icon: Bot },
  ] as const;

  function getTopicState(topicId: string): "pending" | "active" | "completed" {
    if (topicsCovered.includes(topicId)) return "completed";
    if (activeTopic === topicId) return "active";
    return "pending";
  }
</script>

<div class="topic-progress" data-testid="onboarding-progress-bar">
  {#each TOPICS as topic, i}
    {#if i > 0}
      <div
        class="topic-connector"
        class:completed={topicsCovered.includes(TOPICS[i - 1].id)}
      ></div>
    {/if}
    <div
      class="topic-item"
      class:pending={getTopicState(topic.id) === "pending"}
      class:active={getTopicState(topic.id) === "active"}
      class:completed={getTopicState(topic.id) === "completed"}
      data-testid="topic-{topic.id}"
    >
      <div class="topic-circle">
        {#if getTopicState(topic.id) === "completed"}
          <Check size={14} strokeWidth={2.5} class="text-white" />
        {:else}
          <topic.icon size={14} strokeWidth={1.75} />
        {/if}
      </div>
      <span class="topic-label">{$t(`onboarding_v2.acquaintance.topic_${topic.id}`)}</span>
    </div>
  {/each}
</div>

<style>
  .topic-progress {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0;
    padding: 0.75rem 1rem;
  }

  .topic-connector {
    width: 2rem;
    height: 2px;
    background: #9CA3AF;
    transition: background 300ms ease;
  }

  .topic-connector.completed {
    background: #7c5fd6;
  }

  .topic-item {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.375rem;
  }

  .topic-circle {
    width: 2.25rem;
    height: 2.25rem;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 200ms ease;
  }

  .topic-item.pending .topic-circle {
    background: #9CA3AF;
    color: white;
  }

  .topic-item.active .topic-circle {
    background: hsl(var(--primary));
    color: hsl(var(--primary-foreground));
    animation: pulse 1.5s ease-in-out infinite;
    box-shadow: var(--shadow-primary-sm);
  }

  .topic-item.completed .topic-circle {
    background: #7c5fd6;
    color: white;
  }

  .topic-label {
    font-size: 0.6875rem;
    font-weight: 500;
    color: #6B7280;
  }

  .topic-item.active .topic-label {
    color: #3435f5;
    font-weight: 600;
  }

  .topic-item.completed .topic-label {
    color: #7c5fd6;
    font-weight: 600;
  }

  @keyframes pulse {
    0%, 100% { transform: scale(1); }
    50% { transform: scale(1.1); }
  }
</style>
