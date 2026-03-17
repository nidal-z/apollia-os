<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { Button } from "$lib/components/ui/button";
  import { FileText, MessageCircle, Calculator } from "lucide-svelte";
  import type { ComponentType } from "svelte";
  import type { AvailableAgent } from "$lib/types";

  interface Props {
    onAgentStarted: (agentId: string, agentName: string) => void;
    onSkip: () => void;
  }

  let { onAgentStarted, onSkip }: Props = $props();

  /** Catalogue metadata keyed by agent file stem. */
  const CATALOGUE: Record<string, { name: string; description: string; icon: ComponentType }> = {
    "document-analyst": {
      name: "Document Analyst",
      description: "Reads and summarizes your documents",
      icon: FileText,
    },
    "standup-scribe": {
      name: "Standup Scribe",
      description: "Generates daily standup reports from your git history",
      icon: MessageCircle,
    },
    "hello_agent": {
      name: "Hello Agent",
      description: "A simple greeting agent — perfect for testing",
      icon: MessageCircle,
    },
    "devis_agent": {
      name: "Quote Generator",
      description: "Generates price quotes from specifications",
      icon: Calculator,
    },
  };

  interface CatalogueEntry {
    id: string;
    name: string;
    description: string;
    icon: ComponentType;
    path: string;
  }

  let agents = $state<CatalogueEntry[]>([]);
  let loadingAgents = $state(true);
  let selectedId = $state<string | null>(null);
  let starting = $state(false);
  let error = $state<string | null>(null);

  async function loadAgents() {
    try {
      const available = await invoke<AvailableAgent[]>("list_available_agents");
      agents = available.map((a) => {
        const meta = CATALOGUE[a.id];
        return {
          id: a.id,
          name: meta?.name ?? formatAgentName(a.id),
          description: meta?.description ?? "",
          icon: meta?.icon ?? MessageCircle,
          path: a.path,
        };
      });
    } catch {
      agents = [];
    } finally {
      loadingAgents = false;
    }
  }

  function formatAgentName(id: string): string {
    return id
      .replace(/[-_]/g, " ")
      .replace(/\b\w/g, (c) => c.toUpperCase());
  }

  async function selectAgent(entry: CatalogueEntry) {
    selectedId = entry.id;
    starting = true;
    error = null;
    try {
      const id = await invoke<string>("start_agent", { path: entry.path });
      onAgentStarted(id, entry.name);
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
      starting = false;
    }
  }

  async function pickCustomFile() {
    const file = await open({
      title: "Select a Python agent file",
      filters: [{ name: "Python", extensions: ["py"] }],
      multiple: false,
    });
    if (!file) return;

    const path = file as string;
    selectedId = "custom";
    starting = true;
    error = null;
    try {
      const id = await invoke<string>("start_agent", { path });
      const stem = path.split("/").pop()?.replace(".py", "") ?? "agent";
      onAgentStarted(id, formatAgentName(stem));
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
      starting = false;
    }
  }

  onMount(() => {
    loadAgents();
  });
</script>

<div class="space-y-6" data-testid="step-choose-assistant">
  <div>
    <h2 class="text-xl font-semibold">{$t('onboarding.choose_assistant_title')}</h2>
    <p class="mt-1 text-sm text-muted-foreground">
      {$t('onboarding.choose_assistant_subtitle')}
    </p>
  </div>

  {#if loadingAgents}
    <div class="flex items-center justify-center py-8">
      <span class="inline-block h-5 w-5 animate-spin rounded-full border-2 border-primary border-t-transparent"></span>
    </div>
  {:else if agents.length === 0}
    <div class="rounded-lg glass-border border-dashed glass-card p-6 text-center">
      <p class="text-sm text-muted-foreground">{$t('onboarding.no_agents_found')}</p>
    </div>
  {:else}
    <div class="grid gap-3">
      {#each agents as entry (entry.id)}
        {@const Icon = entry.icon}
        <button
          class="flex items-center gap-4 rounded-lg glass-border glass-card p-4 text-left transition-all hover:border-primary hover:shadow-sm
            {selectedId === entry.id ? 'border-primary ring-2 ring-primary/20' : ''}"
          onclick={() => selectAgent(entry)}
          disabled={starting}
          data-testid="agent-card-{entry.id}"
        >
          <span class="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary">
            <Icon size={20} />
          </span>
          <div class="flex-1 min-w-0">
            <p class="text-sm font-semibold">{entry.name}</p>
            {#if entry.description}
              <p class="text-xs text-muted-foreground">{entry.description}</p>
            {/if}
          </div>
          {#if starting && selectedId === entry.id}
            <span class="inline-block h-4 w-4 animate-spin rounded-full border-2 border-primary border-t-transparent"></span>
          {/if}
        </button>
      {/each}
    </div>
  {/if}

  {#if error}
    <div class="rounded-lg border border-[hsl(var(--destructive))] bg-[hsl(var(--destructive))]/10 p-4">
      <p class="text-sm text-[hsl(var(--destructive))]">{error}</p>
    </div>
  {/if}

  <div class="flex items-center justify-between">
    <button
      class="text-xs text-muted-foreground underline-offset-4 hover:underline"
      onclick={pickCustomFile}
      disabled={starting}
    >
      {$t('onboarding.choose_custom_file')}
    </button>
    <Button variant="ghost" onclick={onSkip}>{$t('common.skip')}</Button>
  </div>
</div>
