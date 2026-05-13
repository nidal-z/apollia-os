<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { Download, CheckCircle2, XCircle, RefreshCw } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Card } from "$lib/components/ui/card";
  import { Spinner } from "$lib/components/ui/progress";

  type State = "idle" | "checking" | "up_to_date" | "available" | "error";

  type CheckResult = {
    available: boolean;
    current_version: string;
    new_version?: string | null;
    release_notes?: string | null;
  };

  let phase: State = $state("idle");
  let newVersion: string | null = $state(null);
  let errorMsg: string | null = $state(null);
  let installing = $state(false);

  async function checkForUpdate() {
    phase = "checking";
    errorMsg = null;
    try {
      const result = await invoke<CheckResult>("check_for_update");
      newVersion = result.new_version ?? null;
      phase = result.available ? "available" : "up_to_date";
    } catch (e) {
      phase = "error";
      errorMsg = e instanceof Error ? e.message : String(e);
    }
  }

  async function installUpdate() {
    installing = true;
    errorMsg = null;
    try {
      await invoke("install_update");
    } catch (e) {
      phase = "error";
      errorMsg = e instanceof Error ? e.message : String(e);
    } finally {
      installing = false;
    }
  }
</script>

<Card class="rounded-lg p-4" data-testid="update-checker">
  <h3 class="mb-3 text-sm font-medium uppercase tracking-wider text-muted-foreground">
    {$t("settings.update.check")}
  </h3>

  <div class="flex items-center gap-3">
    {#if phase === "idle"}
      <Button variant="outline" size="sm" onclick={checkForUpdate} data-testid="update-check-btn">
        <RefreshCw class="h-3.5 w-3.5 mr-1.5" />
        {$t("settings.update.check")}
      </Button>
    {:else if phase === "checking"}
      <span class="inline-flex items-center gap-1.5 text-sm text-muted-foreground">
        <Spinner class="h-3.5 w-3.5" />
        {$t("settings.update.checking")}
      </span>
    {:else if phase === "up_to_date"}
      <span class="inline-flex items-center gap-1.5 text-sm">
        <CheckCircle2 class="h-3.5 w-3.5 text-success" />
        {$t("settings.update.up_to_date")}
      </span>
      <Button variant="ghost" size="sm" onclick={checkForUpdate}>
        <RefreshCw class="h-3.5 w-3.5 mr-1.5" />
        {$t("settings.update.check")}
      </Button>
    {:else if phase === "available"}
      <span class="text-sm" data-testid="update-available-label">
        {$t("settings.update.available", { values: { version: newVersion ?? "?" } })}
      </span>
      <Button
        variant="default"
        size="sm"
        onclick={installUpdate}
        disabled={installing}
        data-testid="update-install-btn"
      >
        {#if installing}
          <Spinner class="h-3.5 w-3.5 mr-1.5" />
        {:else}
          <Download class="h-3.5 w-3.5 mr-1.5" />
        {/if}
        {$t("settings.update.install")}
      </Button>
    {:else if phase === "error"}
      <span class="inline-flex items-center gap-1.5 text-sm text-destructive">
        <XCircle class="h-3.5 w-3.5" />
        {$t("settings.update.error")}
      </span>
      <Button variant="ghost" size="sm" onclick={checkForUpdate}>
        <RefreshCw class="h-3.5 w-3.5 mr-1.5" />
        {$t("settings.update.check")}
      </Button>
    {/if}
  </div>

  {#if phase === "error" && errorMsg}
    <p class="text-xs text-destructive mt-2 font-mono break-all">{errorMsg}</p>
  {/if}
</Card>
