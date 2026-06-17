<script lang="ts">
  import type { AgentPackageListItem } from "$lib/types";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Package, AlertTriangle, Users, Info, Trash2 } from "lucide-svelte";
  import { EntityCard } from "$lib/components/operator";

  interface Props {
    pkg: AgentPackageListItem;
    ondetail: (pkg: AgentPackageListItem) => void;
    onuninstall: (pkg: AgentPackageListItem) => void;
  }

  let { pkg, ondetail, onuninstall }: Props = $props();

  let confirmUninstall = $state(false);
  let uninstalling = $state(false);

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

<EntityCard
  accent="primary"
  iconTone="primary"
  title={pkg.name}
  subtitle={`v${pkg.version}`}
  onclick={() => ondetail(pkg)}
>
  {#snippet icon()}
    <Package size={16} class="text-primary" />
  {/snippet}

  {#snippet badges()}
    <Badge variant="outline" class="text-[9px] px-1.5 py-0 gap-0.5">
      <Users size={8} />{pkg.agent_count}
    </Badge>
  {/snippet}

  {#snippet body()}
    {#if pkg.root_missing}
      <div class="flex items-center gap-1 text-[10px] text-destructive/70">
        <AlertTriangle size={10} />
        <span>Dossier source manquant</span>
      </div>
    {/if}

    <!-- Description -->
    {#if pkg.description}
      <p class="line-clamp-2 text-xs text-muted-foreground leading-relaxed">
        {pkg.description}
      </p>
    {/if}

    <!-- Agent pills -->
    <div class="flex flex-wrap gap-1">
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

    <span class="text-[10px] text-muted-foreground/50">
      {new Date(pkg.installed_at).toLocaleDateString()}
    </span>
  {/snippet}

  {#snippet actions()}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
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
  {/snippet}
</EntityCard>
