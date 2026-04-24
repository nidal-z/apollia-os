<script lang="ts">
  import type { AgentPackageDetailView } from "$lib/types";
  import { Sheet, SheetContent, SheetHeader, SheetTitle } from "$lib/components/ui/sheet";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Package, AlertTriangle, Users, Clock, FolderOpen, Trash2, Zap } from "lucide-svelte";

  interface Props {
    pkg: AgentPackageDetailView | null;
    open: boolean;
    onclose: () => void;
    onuninstall: (name: string) => Promise<void>;
  }

  let { pkg, open, onclose, onuninstall }: Props = $props();

  let confirmUninstall = $state(false);
  let uninstalling = $state(false);

  async function handleUninstall() {
    if (!pkg) return;
    uninstalling = true;
    try {
      await onuninstall(pkg.name);
      onclose();
    } finally {
      uninstalling = false;
      confirmUninstall = false;
    }
  }

  const triggers = $derived.by(() => {
    if (!pkg) return [];
    const raw = pkg.manifest as Record<string, unknown>;
    return Array.isArray(raw.triggers) ? raw.triggers : [];
  });
</script>

<Sheet bind:open onOpenChange={(v) => !v && onclose()}>
  <SheetContent side="right" class="w-[420px] sm:w-[480px] overflow-y-auto">
    {#if pkg}
      <SheetHeader class="mb-4">
        <div class="flex items-center gap-3">
          <div class="size-10 rounded-xl bg-primary/10 flex items-center justify-center">
            <Package size={20} class="text-primary" />
          </div>
          <div>
            <SheetTitle class="text-base">{pkg.name}</SheetTitle>
            <p class="text-xs text-muted-foreground">v{pkg.version} · {pkg.author || "Apollia OS"}</p>
          </div>
        </div>
      </SheetHeader>

      {#if pkg.root_missing}
        <div class="mb-4 flex items-center gap-2 rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive">
          <AlertTriangle size={14} />
          <span>Dossier source introuvable — agents désactivés</span>
        </div>
      {/if}

      {#if pkg.description}
        <p class="mb-4 text-sm text-muted-foreground leading-relaxed">{pkg.description}</p>
      {/if}

      <!-- Meta -->
      <div class="mb-4 grid grid-cols-2 gap-2 text-xs text-muted-foreground">
        <div class="flex items-center gap-1.5">
          <Clock size={11} />
          <span>Installé le {new Date(pkg.installed_at).toLocaleDateString()}</span>
        </div>
        <div class="flex items-center gap-1.5 col-span-2 truncate">
          <FolderOpen size={11} class="shrink-0" />
          <span class="truncate" title={pkg.root_path}>{pkg.root_path}</span>
        </div>
      </div>

      <!-- Agents section -->
      <div class="mb-4">
        <h3 class="mb-2 text-xs font-medium text-muted-foreground uppercase tracking-wide flex items-center gap-1.5">
          <Users size={11} />
          Agents ({pkg.agents.length})
        </h3>
        <div class="space-y-1.5">
          {#each pkg.agents as agent (agent.name)}
            <div class="flex items-center justify-between rounded-lg bg-muted/30 px-3 py-2">
              <span class="text-sm font-medium">{agent.name}</span>
              <Badge
                variant={agent.role === "director" ? "default" : "secondary"}
                class="text-[9px] px-1.5 py-0"
              >
                {agent.role}
              </Badge>
            </div>
          {/each}
        </div>
      </div>

      <!-- Triggers section -->
      {#if triggers.length > 0}
        <div class="mb-4">
          <h3 class="mb-2 text-xs font-medium text-muted-foreground uppercase tracking-wide flex items-center gap-1.5">
            <Zap size={11} />
            Triggers ({triggers.length})
          </h3>
          <div class="space-y-1.5">
            {#each triggers as trigger}
              {@const t = trigger as Record<string, unknown>}
              <div class="rounded-lg bg-muted/30 px-3 py-2 text-xs">
                <div class="flex items-center justify-between">
                  <span class="font-medium">{t.id ?? "trigger"}</span>
                  <Badge variant="outline" class="text-[9px] px-1.5 py-0">
                    {(t.source as Record<string, unknown>)?.type ?? "cron"}
                  </Badge>
                </div>
                {#if (t.source as Record<string, unknown>)?.schedule}
                  <p class="mt-0.5 text-muted-foreground font-mono">
                    {(t.source as Record<string, unknown>).schedule}
                  </p>
                {/if}
              </div>
            {/each}
          </div>
        </div>
      {/if}

      <!-- Actions -->
      <div class="mt-6 flex flex-col gap-2">
        {#if confirmUninstall}
          <p class="text-xs text-muted-foreground text-center">
            Désinstaller '{pkg.name}' et ses {pkg.agents.length} agents ?
          </p>
          <div class="flex gap-2">
            <Button
              variant="destructive"
              class="flex-1"
              disabled={uninstalling}
              onclick={handleUninstall}
            >
              {uninstalling ? "Désinstallation…" : "Confirmer"}
            </Button>
            <Button variant="outline" onclick={() => (confirmUninstall = false)}>Annuler</Button>
          </div>
        {:else}
          <Button
            variant="destructive"
            class="w-full gap-2"
            onclick={() => (confirmUninstall = true)}
          >
            <Trash2 size={14} />
            Désinstaller le package
          </Button>
        {/if}
      </div>
    {/if}
  </SheetContent>
</Sheet>
