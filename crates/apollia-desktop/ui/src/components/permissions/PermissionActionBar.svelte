<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { ExternalLink } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Textarea } from "$lib/components/ui/textarea";
  import { addToast } from "$lib/components/ui/toast/store";
  import { setSettingsSubRoute } from "$lib/router";
  import { navigateTo } from "$lib/stores/navigation";

  interface Props {
    /** Identifiant de la tâche suspendue à résoudre. */
    taskId: string;
    /** Nom de l'outil pour la règle de préfixe persistée. */
    toolName: string;
    /** Préfixe d'argument pour la règle "Toujours autoriser" (`undefined` = règle globale). */
    argPrefix?: string;
    /**
     * Chemin canonique du projet courant. Requis pour activer le bouton
     * « Ce projet » ; absent ⇒ le bouton est désactivé avec une explication.
     */
    projectPath?: string;
    /** Appelé une fois la décision confirmée par le runtime. */
    onResolved: () => void;
  }

  let { taskId, toolName, argPrefix, projectPath, onResolved }: Props = $props();

  const MIN_REASON_LENGTH = 10;

  type Scope = "project" | "global";

  let isProcessing = $state(false);
  let pendingScope = $state<Scope | "approve" | "reject" | null>(null);
  let error = $state<string | null>(null);
  let showRejectForm = $state(false);
  let rejectReason = $state("");
  let reasonValid = $derived(rejectReason.trim().length >= MIN_REASON_LENGTH);

  function busy(scope: Scope | "approve" | "reject"): boolean {
    return isProcessing && pendingScope === scope;
  }

  async function handleApprove(): Promise<void> {
    isProcessing = true;
    pendingScope = "approve";
    error = null;
    try {
      await invoke("resume_task", { taskId, approved: true, reason: null });
      onResolved();
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
      isProcessing = false;
      pendingScope = null;
    }
  }

  async function handleReject(): Promise<void> {
    if (!reasonValid) return;
    isProcessing = true;
    pendingScope = "reject";
    error = null;
    try {
      await invoke("resume_task", {
        taskId,
        approved: false,
        reason: rejectReason.trim(),
      });
      onResolved();
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
      isProcessing = false;
      pendingScope = null;
    }
  }

  async function handleAlwaysAllow(scope: Scope): Promise<void> {
    if (scope === "project" && !projectPath) return;
    isProcessing = true;
    pendingScope = scope;
    error = null;
    try {
      await invoke("add_permission_prefix_rule", {
        toolName,
        argPrefix: argPrefix ?? null,
        action: "allow",
        scope,
        projectPath: scope === "project" ? projectPath : null,
      });
      await invoke("resume_task", { taskId, approved: true, reason: null });
      addToast($t("permissions.always_allow_success"), "success", {
        "data-testid": `permission-always-allow-${scope}-toast`,
      });
      onResolved();
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
      isProcessing = false;
      pendingScope = null;
    }
  }

  function openRejectForm(): void {
    showRejectForm = true;
    rejectReason = "";
  }

  function closeRejectForm(): void {
    showRejectForm = false;
    rejectReason = "";
  }

  function openSettingsPermissions(): void {
    setSettingsSubRoute("permissions");
    navigateTo("settings");
  }
</script>

<div class="mt-3 space-y-3">
  {#if error}
    <p class="text-[11px] text-destructive" data-testid="permission-action-error">{error}</p>
  {/if}

  {#if showRejectForm}
    <div class="space-y-1.5">
      <Textarea
        bind:value={rejectReason}
        rows={2}
        placeholder={$t("permissions.reject_reason_placeholder")}
        aria-label={$t("permissions.reject_reason_placeholder")}
        data-testid="permission-reject-reason"
      />
      <p class="text-[10px] text-muted-foreground">
        {rejectReason.trim().length} / {MIN_REASON_LENGTH}
        {$t("permissions.reject_reason_min", { values: { min: MIN_REASON_LENGTH } })}
      </p>
      <div class="flex gap-2">
        <Button
          size="sm"
          variant="destructive"
          class="h-7 px-3 text-[11px]"
          disabled={!reasonValid || isProcessing}
          loading={busy("reject")}
          onclick={handleReject}
          data-testid="permission-reject-confirm"
        >
          {$t("permissions.reject")}
        </Button>
        <Button
          size="sm"
          variant="ghost"
          class="h-7 px-3 text-[11px]"
          disabled={isProcessing}
          onclick={closeRejectForm}
        >
          {$t("common.cancel")}
        </Button>
      </div>
    </div>
  {:else}
    <div class="flex flex-wrap gap-2">
      <Button
        size="sm"
        variant="success"
        class="h-7 px-3 text-[11px]"
        disabled={isProcessing}
        loading={busy("approve")}
        onclick={handleApprove}
        data-testid="permission-approve-btn"
      >
        {$t("permissions.approve")}
      </Button>
      <Button
        size="sm"
        variant="destructive"
        class="h-7 px-3 text-[11px]"
        disabled={isProcessing}
        onclick={openRejectForm}
        data-testid="permission-reject-btn"
      >
        {$t("permissions.reject")}
      </Button>
    </div>

    <div class="space-y-2 rounded-md border border-border bg-muted/30 p-2">
      <p class="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
        Toujours autoriser sans redemander
      </p>
      <div class="grid grid-cols-1 gap-2 sm:grid-cols-2">
        <button
          type="button"
          class="flex h-full flex-col items-start gap-1 rounded-md border border-border bg-background px-3 py-2 text-left text-[11px] transition-colors hover:border-primary/40 hover:bg-muted disabled:cursor-not-allowed disabled:opacity-60"
          disabled={isProcessing || !projectPath}
          onclick={() => handleAlwaysAllow("project")}
          title={projectPath ?? "Aucun projet courant détecté"}
          data-testid="permission-always-allow-project-btn"
        >
          <span class="font-semibold text-foreground">Ce projet</span>
          <span class="text-muted-foreground">
            {#if projectPath}
              Persiste pour ce projet uniquement. Révoquer dans Paramètres.
            {:else}
              Aucun projet courant — utiliser « Cette session » ou « Partout ».
            {/if}
          </span>
          {#if busy("project")}
            <span class="text-[10px] text-primary">Application…</span>
          {/if}
        </button>

        <button
          type="button"
          class="flex h-full flex-col items-start gap-1 rounded-md border border-border bg-background px-3 py-2 text-left text-[11px] transition-colors hover:border-primary/40 hover:bg-muted disabled:cursor-not-allowed disabled:opacity-60"
          disabled={isProcessing}
          onclick={() => handleAlwaysAllow("global")}
          data-testid="permission-always-allow-global-btn"
        >
          <span class="font-semibold text-foreground">Partout</span>
          <span class="text-muted-foreground">
            S'applique à tous les projets. Révoquer dans Paramètres.
          </span>
          {#if busy("global")}
            <span class="text-[10px] text-primary">Application…</span>
          {/if}
        </button>
      </div>
      <button
        type="button"
        class="inline-flex items-center gap-1 text-[11px] text-primary hover:underline"
        onclick={openSettingsPermissions}
        data-testid="permission-open-settings-link"
      >
        <ExternalLink size={11} aria-hidden="true" />
        Gérer les autorisations dans Paramètres
      </button>
    </div>
  {/if}
</div>
