<script lang="ts">
  import type { AgentPackageListItem } from "$lib/types";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Package, AlertTriangle, Users, Zap, Info, Trash2 } from "lucide-svelte";

  interface Props {
    pkg: AgentPackageListItem;
    ondetail: (pkg: AgentPackageListItem) => void;
    onuninstall: (pkg: AgentPackageListItem) => void;
  }

  let { pkg, ondetail, onuninstall }: Props = $props();

  let confirmUninstall = $state(false);
  let uninstalling = $state(false);

  const directorCount = $derived(pkg.agents.filter((a) => a.role === "director").length);
  const workerCount = $derived(pkg.agents.filter((a) => a.role === "worker").length);

  async function handleUninstall() {
    uninstalling = true;
    try {
      await onuninstall(pkg);
    } finally {
      uninstalling = false;
      confirmUninstall = false;
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="glass-card-hover glass-border flex flex-col cursor-pointer rounded-xl overflow-hidden"
  onclick={() => ondetail(pkg)}
>
  <!-- Accent bar -->
  <div class="h-0.5 w-full bg-primary/60"></div>

  <!-- Main content -->
  <div class="px-3.5 pt-3 pb-2.5 flex-1 flex flex-col">
    <!-- Row 1: icon + name + version -->
    <div class="flex items-center gap-2.5">
      <div class="size-8 rounded-lg bg-primary/10 flex items-center justify-center shrink-0">
        <Package size={16} class="text-primary" />
      </div>
      <div class="min-w-0 flex-1">
        <div class="flex items-center gap-2">
          <span class="truncate text-[13px] font-medium">{pkg.name}</span>
          <span class="shrink-0 text-[10px] text-muted-foreground/50">v{pkg.version}</span>
        </div>
        {#if pkg.root_missing}
          <div class="flex items-center gap-1 mt-0.5 text-[10px] text-destructive/70">
            <AlertTriangle size={10} />
            <span>Dossier source manquant</span>
          </div>
        {/if}
      </div>
      <div class="flex items-center gap-1 shrink-0">
        <Badge variant="outline" class="text-[9px] px-1.5 py-0 gap-0.5">
          <Users size={8} />{pkg.agent_count}
        </Badge>
      </div>
    </div>

    <!-- Description -->
    {#if pkg.description}
      <p class="mt-2 line-clamp-2 text-xs text-muted-foreground leading-relaxed flex-1">
        {pkg.description}
      </p>
    {/if}

    <!-- Agent pills -->
    <div class="mt-2.5 flex flex-wrap gap-1">
      {#each pkg.agents as agent (agent.name)}
        <span
          class="rounded-full px-2 py-px text-[10px] {agent.role === 'director'
            ? 'bg-primary/10 text-primary/80'
            : 'bg-secondary/10 text-secondary/70'}"
          title={agent.role}
        >
          {#if agent.role === "director"}
            ⬡
          {:else}
            ⬢
          {/if}
          {agent.name}
        </span>
      {/each}
    </div>

    <!-- Footer row -->
    <div class="mt-3 flex items-center justify-between gap-2 pt-2 border-t border-border/30">
      <span class="text-[10px] text-muted-foreground/50">
        {new Date(pkg.installed_at).toLocaleDateString()}
      </span>
      <div class="flex items-center gap-1" onclick={(e) => e.stopPropagation()}>
        <Button
          size="icon"
          variant="ghost"
          class="size-6"
          title="Détails"
          onclick={() => ondetail(pkg)}
        >
          <Info size={13} />
        </Button>
        {#if confirmUninstall}
          <Button
            size="sm"
            variant="destructive"
            class="h-6 text-[10px] px-2"
            disabled={uninstalling}
            onclick={handleUninstall}
          >
            {uninstalling ? "…" : "Confirmer"}
          </Button>
          <Button
            size="sm"
            variant="ghost"
            class="h-6 text-[10px] px-2"
            onclick={() => (confirmUninstall = false)}
          >
            Annuler
          </Button>
        {:else}
          <Button
            size="icon"
            variant="ghost"
            class="size-6 text-destructive/60 hover:text-destructive"
            title="Désinstaller"
            onclick={() => (confirmUninstall = true)}
          >
            <Trash2 size={13} />
          </Button>
        {/if}
      </div>
    </div>
  </div>
</div>
