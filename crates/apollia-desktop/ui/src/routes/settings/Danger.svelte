<script lang="ts" context="module">
  export const meta = {
    title: "settings.nav.danger",
    icon: "alert-triangle",
    group: "settings.nav.cluster_danger",
    cluster: "danger",
  } as const;
</script>

<script lang="ts">
  import { onMount, tick } from "svelte";
  import { t } from "svelte-i18n";
  import { AlertTriangle, RefreshCcw, Brain, FileText, Trash2, Ban } from "lucide-svelte";
  import { cn } from "$lib/utils";
  import { Button } from "$lib/components/ui/button";
  import { addToast } from "$lib/components/ui/toast";
  import { reportError } from "$lib/errors/reportError";
  import { onboardingStore, onboardingModalOpen } from "$lib/stores/onboarding";
  import {
    resetOnboarding,
    clearAllMemories,
    clearLogs,
    factoryReset,
    appRestart,
  } from "$lib/ipc/danger";
  import { resetTourState } from "$lib/tour/persistence";
  import SettingsSubPage from "../../components/settings/SettingsSubPage.svelte";
  import DestructiveActionDialog from "../../components/settings/DestructiveActionDialog.svelte";

  type ActionId =
    | "reset-onboarding"
    | "clear-memories"
    | "clear-logs"
    | "factory-reset";

  interface DangerAction {
    id: ActionId;
    /** i18n stem segment under `settings.danger` (underscored). */
    key: string;
    icon: typeof RefreshCcw;
    /** Word the operator must type to unlock the confirm button. */
    confirmWord: string;
    irreversible: boolean;
    /** Last-resort action: heavier card weight, isolated under a divider. */
    critical: boolean;
    /** Extra danger banner shown inside the confirmation dialog. */
    warning: boolean;
    /** Confirm stays locked for this many ms after step 2 opens. */
    lockMs: number;
    run: () => Promise<void>;
  }

  let openAction = $state<ActionId | null>(null);
  let loading = $state(false);

  async function runResetOnboarding(): Promise<void> {
    loading = true;
    try {
      await resetOnboarding();
      // The tour and its milestones live in the frontend, so the backend reset
      // does not reach them. Clearing them here is what makes "reset
      // onboarding" mean the same thing as a fresh install.
      resetTourState();
      // Reload the now-empty state so derived stores (badges, etc.) catch up.
      await onboardingStore.refreshState();
      // Open the onboarding modal immediately, no restart required.
      onboardingModalOpen.set(true);
      addToast($t("settings.danger.reset_onboarding.success"), "success", {
        "data-testid": "danger-reset-onboarding-toast",
      });
      openAction = null;
    } catch (err) {
      reportError(err, { surface: "toast" });
    } finally {
      loading = false;
    }
  }

  async function runClearMemories(): Promise<void> {
    loading = true;
    try {
      const count = await clearAllMemories();
      addToast(
        $t("settings.danger.clear_memories.success", { values: { count } }),
        "success",
        { "data-testid": "danger-clear-memories-toast" },
      );
      openAction = null;
    } catch (err) {
      reportError(err, { surface: "toast" });
    } finally {
      loading = false;
    }
  }

  async function runClearLogs(): Promise<void> {
    loading = true;
    try {
      await clearLogs();
      addToast($t("settings.danger.clear_logs.success"), "success", {
        "data-testid": "danger-clear-logs-toast",
      });
      openAction = null;
    } catch (err) {
      reportError(err, { surface: "toast" });
    } finally {
      loading = false;
    }
  }

  async function runFactoryReset(): Promise<void> {
    loading = true;
    try {
      await factoryReset();
      openAction = null;
      // Factory reset wipes the config dir, so a restart is mandatory to
      // re-init the runtime. In dev mode app_restart may fail (no packaged
      // bundle), so fall back to a persistent manual-restart prompt.
      try {
        await appRestart();
      } catch {
        addToast($t("settings.danger.factory_reset.restart_manual"), "warning", {
          duration: 0,
        });
        loading = false;
      }
    } catch (err) {
      reportError(err, { surface: "toast" });
      loading = false;
    }
  }

  const ACTIONS: DangerAction[] = [
    { id: "reset-onboarding", key: "reset_onboarding", icon: RefreshCcw, confirmWord: "RESET", irreversible: false, critical: false, warning: false, lockMs: 0, run: runResetOnboarding },
    { id: "clear-memories", key: "clear_memories", icon: Brain, confirmWord: "DELETE", irreversible: true, critical: false, warning: false, lockMs: 0, run: runClearMemories },
    { id: "clear-logs", key: "clear_logs", icon: FileText, confirmWord: "DELETE", irreversible: false, critical: false, warning: false, lockMs: 0, run: runClearLogs },
    { id: "factory-reset", key: "factory_reset", icon: Trash2, confirmWord: "FACTORY RESET", irreversible: true, critical: true, warning: true, lockMs: 3000, run: runFactoryReset },
  ];

  // Anchor scroll (?focus=reset-onboarding from UserMenu).
  onMount(async () => {
    await tick();
    const params = new URLSearchParams(window.location.search);
    const focus = params.get("focus");
    if (focus) {
      const el = document.getElementById(focus);
      el?.scrollIntoView({ behavior: "smooth", block: "center" });
      (el?.querySelector("button") as HTMLButtonElement | null)?.focus();
    }
  });
</script>

{#snippet card(action: DangerAction)}
  {@const Icon = action.icon}
  <article
    id={action.id}
    class={cn(
      "rounded-xl",
      action.critical
        ? "border-2 border-destructive/60 bg-destructive/10 p-5"
        : "border border-destructive/30 bg-destructive/5 p-4 transition-colors hover:border-destructive/45",
    )}
    data-testid={`danger-card-${action.id}`}
  >
    <div class="flex flex-wrap items-start justify-between gap-4">
      <div class="flex min-w-0 items-start gap-3">
        <Icon
          size={action.critical ? 20 : 18}
          strokeWidth={1.75}
          class="mt-0.5 shrink-0 text-destructive"
          aria-hidden="true"
        />
        <div class="min-w-0 space-y-1">
          <h2 class={cn("font-semibold text-foreground", action.critical ? "text-body-md" : "text-body-sm")}>
            {$t(`settings.danger.${action.key}.title`)}
          </h2>
          <p class="text-caption text-muted-foreground">
            {$t(`settings.danger.${action.key}.description`)}
          </p>
          {#if action.irreversible}
            <span class="mt-1 inline-flex items-center gap-1 rounded-full border border-destructive/25 bg-destructive/10 px-2 py-0.5 text-overline uppercase text-destructive">
              <Ban size={11} strokeWidth={2} aria-hidden="true" />
              {$t("settings.danger.irreversible")}
            </span>
          {/if}
        </div>
      </div>
      <Button
        variant="destructive"
        size={action.critical ? "default" : "sm"}
        onclick={() => (openAction = action.id)}
        data-testid={`${action.id}-btn`}
      >
        {$t(`settings.danger.${action.key}.button`)}
      </Button>
    </div>
  </article>
{/snippet}

<SettingsSubPage route="danger" data-testid="danger-section">
  <div
    class="flex items-start gap-3 rounded-lg border border-warning/40 bg-warning/5 px-4 py-3"
    role="note"
    data-testid="danger-warning-banner"
  >
    <AlertTriangle size={18} strokeWidth={1.75} class="mt-0.5 shrink-0 text-warning" aria-hidden="true" />
    <p class="text-body-sm text-foreground">{$t("settings.danger.banner")}</p>
  </div>

  {#each ACTIONS as action (action.id)}
    {#if action.critical}
      <div class="space-y-2 border-t border-destructive/40 pt-4">
        <p class="inline-flex items-center gap-1.5 text-overline uppercase text-destructive">
          <AlertTriangle size={13} strokeWidth={2} aria-hidden="true" />
          {$t("settings.danger.last_resort")}
        </p>
        {@render card(action)}
      </div>
    {:else}
      {@render card(action)}
    {/if}
  {/each}
</SettingsSubPage>

{#each ACTIONS as action (action.id)}
  <DestructiveActionDialog
    open={openAction === action.id}
    title={$t(`settings.danger.${action.key}.dialog_title`)}
    description={$t(`settings.danger.${action.key}.dialog_desc`)}
    warning={action.warning ? $t("settings.danger.factory_reset.warning") : undefined}
    confirmWord={action.confirmWord}
    confirmLabel={$t(`settings.danger.${action.key}.button`)}
    lockMs={action.lockMs}
    {loading}
    onclose={() => (openAction = null)}
    onconfirm={action.run}
    data-testid={`${action.id}-confirm`}
  />
{/each}
