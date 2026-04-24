<script lang="ts">
  /**
   * US-SP42-029 — Prominent A2A badge.
   *
   * Surfaces the pool of active worker agents that the current conversation can
   * delegate to. Click opens a popover listing each worker with its live status
   * and declared skills (as colored chips).
   *
   * Data sources:
   *   - `list_agents` Tauri command — enumerates agents + runtime status.
   *   - `list_a2a_skills` — skills declared by worker agents.
   *   - Optional Tauri event `a2a:worker_status` — pushed live status updates.
   *
   * Graceful degradation: if the event is not emitted, the badge still refreshes
   * on mount and when the popover opens.
   */
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { t } from "svelte-i18n";
  import { Users, ChevronDown, Dot } from "lucide-svelte";
  import { Popover } from "$lib/components/ui/popover";
  import type { AgentListItem, A2ASkillListing } from "$lib/types";
  import A2AWorkerSkillChip from "./A2AWorkerSkillChip.svelte";

  let open = $state(false);
  let agents = $state<AgentListItem[]>([]);
  let skills = $state<A2ASkillListing[]>([]);
  let loading = $state(false);
  let unlisten: UnlistenFn | undefined;

  const workers = $derived(
    agents.filter((a) => a.supports_a2a === true && a.agent_type === "worker"),
  );

  const activeWorkers = $derived(
    workers.filter((w) => w.runtime_status === "active"),
  );

  const skillsByAgent = $derived.by(() => {
    const map = new Map<string, A2ASkillListing[]>();
    for (const s of skills) {
      if (!map.has(s.agent_name)) map.set(s.agent_name, []);
      map.get(s.agent_name)!.push(s);
    }
    return map;
  });

  async function refresh(): Promise<void> {
    loading = true;
    try {
      const [a, s] = await Promise.all([
        invoke<AgentListItem[]>("list_agents").catch(() => [] as AgentListItem[]),
        invoke<A2ASkillListing[]>("list_a2a_skills").catch(() => [] as A2ASkillListing[]),
      ]);
      agents = a;
      skills = s;
    } finally {
      loading = false;
    }
  }

  onMount(async () => {
    await refresh();
    try {
      unlisten = await listen<unknown>("a2a:worker_status", () => {
        void refresh();
      });
    } catch {
      // Event bus unavailable — manual refresh only.
    }
  });

  onDestroy(() => {
    unlisten?.();
  });

  function statusTone(status: AgentListItem["runtime_status"]): string {
    switch (status) {
      case "active":
        return "text-success";
      case "degraded":
        return "text-warning";
      case "initializing":
      case "stopping":
        return "text-info";
      default:
        return "text-muted-foreground/50";
    }
  }

  $effect(() => {
    if (open) void refresh();
  });
</script>

<Popover bind:open side="bottom" align="end" class="p-0 min-w-[18rem]">
  {#snippet trigger(triggerProps: Record<string, unknown>)}
    <button
      {...triggerProps}
      class="inline-flex items-center gap-1 rounded-full border border-secondary/30 bg-secondary/10 px-2 py-0.5 text-[10px] font-medium text-secondary hover:bg-secondary/20 transition-colors"
      aria-label={$t("chat.a2a_workers")}
      data-testid="a2a-worker-badge"
    >
      <Users size={11} class="shrink-0" />
      <span>{activeWorkers.length}</span>
      <span class="hidden sm:inline text-secondary/70">{$t("chat.a2a_workers")}</span>
      <ChevronDown size={9} class="opacity-70" />
    </button>
  {/snippet}
  {#snippet content()}
    <div class="p-2 max-h-80 overflow-y-auto" data-testid="a2a-worker-popover">
      <p class="px-2 pb-1.5 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground/60">
        {$t("chat.a2a_workers")}
      </p>
      {#if loading && workers.length === 0}
        <p class="px-2 py-3 text-[11px] text-muted-foreground">{$t("common.loading")}</p>
      {:else if workers.length === 0}
        <p class="px-2 py-3 text-[11px] text-muted-foreground/70">{$t("chat.a2a_no_workers")}</p>
      {:else}
        <ul class="space-y-2">
          {#each workers as worker (worker.name)}
            {@const workerSkills = skillsByAgent.get(worker.name) ?? []}
            <li
              class="rounded-md border border-border/30 px-2 py-1.5"
              data-testid="a2a-worker-row-{worker.name}"
            >
              <div class="flex items-center gap-1.5">
                <Dot
                  size={18}
                  class="-m-1 shrink-0 {statusTone(worker.runtime_status)}"
                />
                <span class="flex-1 truncate text-[12px] font-medium text-foreground">
                  {worker.name}
                </span>
                <span class="shrink-0 text-[9px] uppercase tracking-wider {statusTone(worker.runtime_status)}">
                  {worker.runtime_status ?? $t("chat.a2a_worker_stopped")}
                </span>
              </div>
              {#if worker.description}
                <p class="mt-0.5 truncate text-[10px] text-muted-foreground/80">
                  {worker.description}
                </p>
              {/if}
              {#if workerSkills.length > 0}
                <div class="mt-1 flex flex-wrap gap-1">
                  {#each workerSkills as sk (sk.skill_id)}
                    <A2AWorkerSkillChip
                      skillId={sk.skill_id}
                      skillName={sk.skill_name}
                      description={sk.description}
                      agentName={sk.agent_name}
                    />
                  {/each}
                </div>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  {/snippet}
</Popover>
