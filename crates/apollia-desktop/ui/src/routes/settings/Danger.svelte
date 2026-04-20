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
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { AlertTriangle, RefreshCcw, Brain, FileText, Trash2 } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { addToast } from "$lib/components/ui/toast";
  import { navigateTo } from "$lib/stores/navigation";
  import { onboardingStore } from "$lib/stores/onboarding";
  import DestructiveActionDialog from "../../components/settings/DestructiveActionDialog.svelte";

  type ActionId = "reset-onboarding" | "clear-memories" | "clear-logs" | "factory-reset";

  let openAction = $state<ActionId | null>(null);
  let loading = $state(false);

  async function runResetOnboarding() {
    loading = true;
    try {
      await invoke("reset_onboarding");
      onboardingStore.setRequired?.();
      addToast($t("settings.danger.reset_onboarding.success"), "success", {
        "data-testid": "danger-reset-onboarding-toast",
      });
      openAction = null;
      if (confirm($t("settings.danger.restart_prompt"))) {
        await invoke("app_restart").catch(() => {});
      } else {
        navigateTo("onboarding");
      }
    } catch (err) {
      addToast(String(err), "error");
    } finally {
      loading = false;
    }
  }

  async function runClearMemories() {
    loading = true;
    try {
      const count = await invoke<number>("clear_user_memory");
      addToast(
        $t("settings.danger.clear_memories.success", { values: { count } }),
        "success",
        { "data-testid": "danger-clear-memories-toast" },
      );
      openAction = null;
    } catch (err) {
      addToast(String(err), "error");
    } finally {
      loading = false;
    }
  }

  async function runClearLogs() {
    loading = true;
    try {
      await invoke("clear_logs");
      addToast($t("settings.danger.clear_logs.success"), "success", {
        "data-testid": "danger-clear-logs-toast",
      });
      openAction = null;
    } catch (err) {
      addToast(String(err), "error");
    } finally {
      loading = false;
    }
  }

  async function runFactoryReset() {
    loading = true;
    try {
      await invoke("factory_reset");
      openAction = null;
      // Factory reset wipes the config dir — a restart is mandatory to
      // re-init the runtime.
      await invoke("app_restart").catch(() => {});
    } catch (err) {
      addToast(String(err), "error");
      loading = false;
    }
  }

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

<section class="space-y-6" data-testid="danger-section">
  <header class="space-y-2">
    <h1 class="text-xl font-semibold">{$t("settings.danger.title")}</h1>
    <div
      class="flex items-start gap-3 rounded-md border border-amber-500/40 bg-amber-500/5 px-4 py-3"
      role="note"
      data-testid="danger-warning-banner"
    >
      <AlertTriangle
        size={18}
        strokeWidth={1.75}
        class="mt-0.5 flex-shrink-0 text-amber-600 dark:text-amber-400"
        aria-hidden="true"
      />
      <p class="text-sm text-amber-900 dark:text-amber-200">
        {$t("settings.danger.banner")}
      </p>
    </div>
  </header>

  <!-- Reset Onboarding -->
  <article
    id="reset-onboarding"
    class="rounded-lg border border-destructive/30 bg-destructive/5 p-4"
    data-testid="danger-card-reset-onboarding"
  >
    <div class="flex items-start justify-between gap-4">
      <div class="flex items-start gap-3">
        <RefreshCcw size={18} strokeWidth={1.75} class="mt-0.5 text-destructive" aria-hidden="true" />
        <div>
          <h2 class="text-sm font-medium text-foreground">
            {$t("settings.danger.reset_onboarding.title")}
          </h2>
          <p class="text-xs text-muted-foreground">
            {$t("settings.danger.reset_onboarding.description")}
          </p>
        </div>
      </div>
      <Button
        variant="destructive"
        size="sm"
        onclick={() => { openAction = "reset-onboarding"; }}
        data-testid="reset-onboarding-btn"
      >
        {$t("settings.danger.reset_onboarding.button")}
      </Button>
    </div>
  </article>

  <!-- Clear Memories -->
  <article
    id="clear-memories"
    class="rounded-lg border border-destructive/30 bg-destructive/5 p-4"
    data-testid="danger-card-clear-memories"
  >
    <div class="flex items-start justify-between gap-4">
      <div class="flex items-start gap-3">
        <Brain size={18} strokeWidth={1.75} class="mt-0.5 text-destructive" aria-hidden="true" />
        <div>
          <h2 class="text-sm font-medium text-foreground">
            {$t("settings.danger.clear_memories.title")}
          </h2>
          <p class="text-xs text-muted-foreground">
            {$t("settings.danger.clear_memories.description")}
          </p>
        </div>
      </div>
      <Button
        variant="destructive"
        size="sm"
        onclick={() => { openAction = "clear-memories"; }}
        data-testid="clear-memories-btn"
      >
        {$t("settings.danger.clear_memories.button")}
      </Button>
    </div>
  </article>

  <!-- Clear Logs -->
  <article
    id="clear-logs"
    class="rounded-lg border border-destructive/30 bg-destructive/5 p-4"
    data-testid="danger-card-clear-logs"
  >
    <div class="flex items-start justify-between gap-4">
      <div class="flex items-start gap-3">
        <FileText size={18} strokeWidth={1.75} class="mt-0.5 text-destructive" aria-hidden="true" />
        <div>
          <h2 class="text-sm font-medium text-foreground">
            {$t("settings.danger.clear_logs.title")}
          </h2>
          <p class="text-xs text-muted-foreground">
            {$t("settings.danger.clear_logs.description")}
          </p>
        </div>
      </div>
      <Button
        variant="destructive"
        size="sm"
        onclick={() => { openAction = "clear-logs"; }}
        data-testid="clear-logs-btn"
      >
        {$t("settings.danger.clear_logs.button")}
      </Button>
    </div>
  </article>

  <!-- Factory Reset — separated with an extra warning -->
  <div class="space-y-2 pt-4 border-t border-destructive/40">
    <p class="text-xs font-medium uppercase tracking-wider text-destructive">
      {$t("settings.danger.last_resort")}
    </p>
    <article
      id="factory-reset"
      class="rounded-lg border-2 border-destructive/60 bg-destructive/10 p-5"
      data-testid="danger-card-factory-reset"
    >
      <div class="flex items-start justify-between gap-4">
        <div class="flex items-start gap-3">
          <Trash2 size={20} strokeWidth={1.75} class="mt-0.5 text-destructive" aria-hidden="true" />
          <div>
            <h2 class="text-base font-semibold text-foreground">
              {$t("settings.danger.factory_reset.title")}
            </h2>
            <p class="text-xs text-muted-foreground">
              {$t("settings.danger.factory_reset.description")}
            </p>
          </div>
        </div>
        <Button
          variant="destructive"
          onclick={() => { openAction = "factory-reset"; }}
          data-testid="factory-reset-btn"
        >
          {$t("settings.danger.factory_reset.button")}
        </Button>
      </div>
    </article>
  </div>
</section>

<DestructiveActionDialog
  open={openAction === "reset-onboarding"}
  title={$t("settings.danger.reset_onboarding.dialog_title")}
  description={$t("settings.danger.reset_onboarding.dialog_desc")}
  confirmWord="RESET"
  confirmLabel={$t("settings.danger.reset_onboarding.button")}
  {loading}
  onclose={() => { openAction = null; }}
  onconfirm={runResetOnboarding}
  data-testid="reset-onboarding-confirm"
/>

<DestructiveActionDialog
  open={openAction === "clear-memories"}
  title={$t("settings.danger.clear_memories.dialog_title")}
  description={$t("settings.danger.clear_memories.dialog_desc")}
  confirmWord="DELETE"
  confirmLabel={$t("settings.danger.clear_memories.button")}
  {loading}
  onclose={() => { openAction = null; }}
  onconfirm={runClearMemories}
  data-testid="clear-memories-confirm"
/>

<DestructiveActionDialog
  open={openAction === "clear-logs"}
  title={$t("settings.danger.clear_logs.dialog_title")}
  description={$t("settings.danger.clear_logs.dialog_desc")}
  confirmWord="DELETE"
  confirmLabel={$t("settings.danger.clear_logs.button")}
  {loading}
  onclose={() => { openAction = null; }}
  onconfirm={runClearLogs}
  data-testid="clear-logs-confirm"
/>

<DestructiveActionDialog
  open={openAction === "factory-reset"}
  title={$t("settings.danger.factory_reset.dialog_title")}
  description={$t("settings.danger.factory_reset.dialog_desc")}
  warning={$t("settings.danger.factory_reset.warning")}
  confirmWord="FACTORY RESET"
  confirmLabel={$t("settings.danger.factory_reset.button")}
  lockMs={3000}
  {loading}
  onclose={() => { openAction = null; }}
  onconfirm={runFactoryReset}
  data-testid="factory-reset-confirm"
/>
