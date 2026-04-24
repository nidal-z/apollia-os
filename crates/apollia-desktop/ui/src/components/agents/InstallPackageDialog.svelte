<script lang="ts">
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import type { PackagePreview, InstallPackageResponse } from "$lib/types";
  import { previewPackage, installPackage } from "$lib/stores/agentPackages";
  import { Button } from "$lib/components/ui/button";
  import { Badge } from "$lib/components/ui/badge";
  import { Dialog, DialogContent, DialogHeader, DialogTitle } from "$lib/components/ui/dialog";
  import { Package, FolderOpen, AlertTriangle, CheckCircle2, Users, Zap, Package2 } from "lucide-svelte";

  interface Props {
    open: boolean;
    onclose: () => void;
    oninstalled: (result: InstallPackageResponse) => void;
  }

  let { open, onclose, oninstalled }: Props = $props();

  type Step = "pick" | "preview" | "installing" | "done";

  let step = $state<Step>("pick");
  let selectedPath = $state<string | null>(null);
  let preview = $state<PackagePreview | null>(null);
  let previewLoading = $state(false);
  let previewError = $state<string | null>(null);
  let installError = $state<string | null>(null);
  let installResult = $state<InstallPackageResponse | null>(null);

  function reset() {
    step = "pick";
    selectedPath = null;
    preview = null;
    previewLoading = false;
    previewError = null;
    installError = null;
    installResult = null;
  }

  function handleClose() {
    reset();
    onclose();
  }

  async function handlePickFolder() {
    const result = await openDialog({ directory: true, multiple: false });
    if (!result) return;
    const path = typeof result === "string" ? result : result[0];
    if (!path) return;

    selectedPath = path;
    previewLoading = true;
    previewError = null;
    preview = null;

    try {
      preview = await previewPackage(path);
      step = "preview";
    } catch (e) {
      previewError = String(e);
    } finally {
      previewLoading = false;
    }
  }

  async function handleInstall() {
    if (!selectedPath) return;
    step = "installing";
    installError = null;

    try {
      installResult = await installPackage(selectedPath);
      step = "done";
      oninstalled(installResult!);
    } catch (e) {
      installError = String(e);
      step = "preview";
    }
  }
</script>

<Dialog bind:open onOpenChange={(v) => !v && handleClose()}>
  <DialogContent class="sm:max-w-lg">
    <DialogHeader>
      <DialogTitle class="flex items-center gap-2">
        <Package size={16} class="text-primary" />
        Installer un package d'agents
      </DialogTitle>
    </DialogHeader>

    <!-- Step: pick folder -->
    {#if step === "pick"}
      <div class="py-4 flex flex-col items-center gap-4 text-center">
        {#if previewLoading}
          <div class="size-16 rounded-2xl bg-primary/10 flex items-center justify-center animate-pulse">
            <Package2 size={28} class="text-primary/60" />
          </div>
          <p class="text-sm text-muted-foreground">Analyse du package…</p>
        {:else}
          <div class="size-16 rounded-2xl bg-primary/10 flex items-center justify-center">
            <FolderOpen size={28} class="text-primary" />
          </div>
          <div>
            <p class="text-sm font-medium">Sélectionner un dossier package</p>
            <p class="text-xs text-muted-foreground mt-1">
              Le dossier doit contenir un fichier <code class="bg-muted px-1 rounded">agent.toml</code>
            </p>
          </div>

          {#if previewError}
            <div class="w-full flex items-start gap-2 rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive text-left">
              <AlertTriangle size={13} class="mt-0.5 shrink-0" />
              <span>{previewError}</span>
            </div>
          {/if}

          <Button onclick={handlePickFolder} class="gap-2 w-full">
            <FolderOpen size={14} />
            Choisir un dossier
          </Button>
        {/if}
      </div>

    <!-- Step: preview -->
    {:else if step === "preview" && preview}
      <div class="py-2 space-y-4">
        <!-- Package header -->
        <div class="flex items-center gap-3">
          <div class="size-10 rounded-xl bg-primary/10 flex items-center justify-center shrink-0">
            <Package size={18} class="text-primary" />
          </div>
          <div>
            <p class="font-medium text-sm">{preview.name}</p>
            <p class="text-xs text-muted-foreground">v{preview.version} · {preview.author || "—"}</p>
          </div>
          <div class="ml-auto">
            {#if preview.valid}
              <Badge variant="success" class="text-[10px] px-2 py-0.5 gap-1">
                <CheckCircle2 size={10} />Valide
              </Badge>
            {:else}
              <Badge variant="destructive" class="text-[10px] px-2 py-0.5 gap-1">
                <AlertTriangle size={10} />Invalide
              </Badge>
            {/if}
          </div>
        </div>

        {#if preview.description}
          <p class="text-xs text-muted-foreground leading-relaxed">{preview.description}</p>
        {/if}

        {#if preview.error}
          <div class="flex items-start gap-2 rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive">
            <AlertTriangle size={13} class="mt-0.5 shrink-0" />
            <span>{preview.error}</span>
          </div>
        {/if}

        <!-- Agents -->
        <div>
          <h4 class="text-[10px] font-medium uppercase text-muted-foreground tracking-wide mb-1.5 flex items-center gap-1">
            <Users size={10} />Agents ({preview.agents.length})
          </h4>
          <div class="space-y-1">
            {#each preview.agents as agent (agent.name)}
              <div class="flex items-center justify-between rounded-lg bg-muted/30 px-2.5 py-1.5 text-xs">
                <span class="font-medium">{agent.name}</span>
                <Badge variant={agent.role === "director" ? "default" : "secondary"} class="text-[9px] px-1.5 py-0">
                  {agent.role}
                </Badge>
              </div>
            {/each}
          </div>
        </div>

        <!-- Triggers + pip -->
        {#if preview.trigger_count > 0 || preview.pip_packages.length > 0}
          <div class="flex gap-3 text-xs text-muted-foreground">
            {#if preview.trigger_count > 0}
              <span class="flex items-center gap-1">
                <Zap size={10} class="text-primary" />
                {preview.trigger_count} trigger{preview.trigger_count > 1 ? "s" : ""}
              </span>
            {/if}
            {#if preview.pip_packages.length > 0}
              <span class="flex items-center gap-1">
                <Package2 size={10} />
                {preview.pip_packages.length} dépendance{preview.pip_packages.length > 1 ? "s" : ""} pip
              </span>
            {/if}
          </div>
        {/if}

        {#if installError}
          <div class="flex items-start gap-2 rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive">
            <AlertTriangle size={13} class="mt-0.5 shrink-0" />
            <span>{installError}</span>
          </div>
        {/if}

        <div class="flex gap-2 pt-1">
          <Button variant="outline" onclick={reset} class="flex-1">← Changer</Button>
          <Button
            onclick={handleInstall}
            class="flex-1"
            disabled={!preview.valid}
          >
            Installer
          </Button>
        </div>
      </div>

    <!-- Step: installing -->
    {:else if step === "installing"}
      <div class="py-8 flex flex-col items-center gap-3 text-center">
        <div class="size-16 rounded-2xl bg-primary/10 flex items-center justify-center animate-pulse">
          <Package size={28} class="text-primary/60" />
        </div>
        <p class="text-sm font-medium">Installation en cours…</p>
        <p class="text-xs text-muted-foreground">Copie des fichiers et injection des triggers</p>
      </div>

    <!-- Step: done -->
    {:else if step === "done" && installResult}
      <div class="py-6 flex flex-col items-center gap-4 text-center">
        <div class="size-16 rounded-2xl bg-success/10 flex items-center justify-center">
          <CheckCircle2 size={28} class="text-success" />
        </div>
        <div>
          <p class="text-sm font-medium">Package installé !</p>
          <p class="text-xs text-muted-foreground mt-1">
            <span class="font-medium">{installResult.name}</span> v{installResult.version} ·
            {installResult.agent_count} agent{installResult.agent_count > 1 ? "s" : ""} ·
            {installResult.trigger_count} trigger{installResult.trigger_count > 1 ? "s" : ""}
          </p>
        </div>
        <Button onclick={handleClose} class="w-full">Fermer</Button>
      </div>
    {/if}
  </DialogContent>
</Dialog>
