<script lang="ts">
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { t } from "svelte-i18n";
  import { get } from "svelte/store";
  import type { PackagePreview, InstallPackageResponse, TriggerConfigOverride, TriggerPreview } from "$lib/types";
  import { previewPackage, installPackage } from "$lib/stores/agentPackages";
  import { agentInstallPrefs } from "$lib/stores/agentInstallPrefs";
  import { Button } from "$lib/components/ui/button";
  import { Badge } from "$lib/components/ui/badge";
  import { Dialog } from "$lib/components/ui/dialog";
  import { FormField } from "$lib/components/ui/form-field";
  import {
    Package,
    FolderOpen,
    AlertTriangle,
    CheckCircle2,
    Users,
    Zap,
    Package2,
    Webhook,
    Clock,
    Timer,
    Eye,
    Settings,
    Copy,
  } from "lucide-svelte";

  interface Props {
    open: boolean;
    onclose: () => void;
    oninstalled: (result: InstallPackageResponse) => void;
  }

  let { open, onclose, oninstalled }: Props = $props();

  type Step = "pick" | "preview" | "deps-confirm" | "configure" | "installing" | "done";

  let step = $state<Step>("pick");
  let selectedPath = $state<string | null>(null);
  let preview = $state<PackagePreview | null>(null);
  let previewLoading = $state(false);
  let previewError = $state<string | null>(null);
  let installError = $state<string | null>(null);
  let installResult = $state<InstallPackageResponse | null>(null);
  let depsConfirmed = $state(false);

  // Webhook config state: map from trigger id → secret value
  let webhookSecrets = $state<Record<string, string>>({});
  let copiedId = $state<string | null>(null);

  const webhookTriggers = $derived(
    preview?.triggers.filter((t) => t.source_type === "webhook") ?? [],
  );
  const needsConfig = $derived(webhookTriggers.some((t) => t.needs_config));

  function reset() {
    step = "pick";
    selectedPath = null;
    preview = null;
    previewLoading = false;
    previewError = null;
    installError = null;
    installResult = null;
    webhookSecrets = {};
    copiedId = null;
    depsConfirmed = false;
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
      // Pre-populate secrets if already provided in the manifest.
      for (const t of preview.triggers) {
        if (t.source_type === "webhook") {
          webhookSecrets[t.id] = webhookSecrets[t.id] ?? "";
        }
      }
      step = "preview";
    } catch (e) {
      previewError = String(e);
    } finally {
      previewLoading = false;
    }
  }

  const hasPipDeps = $derived((preview?.pip_packages?.length ?? 0) > 0);
  const willShowDepsConfirm = $derived(
    hasPipDeps && !depsConfirmed && !$agentInstallPrefs.autoInstallPythonDeps,
  );

  function proceedFromPreview() {
    // Si l'utilisateur a activé l'auto-install dans Settings → System,
    // on saute l'étape de confirmation des deps et on transmet directement
    // depsConfirmed=true au backend.
    const autoInstall = get(agentInstallPrefs).autoInstallPythonDeps;
    if (hasPipDeps && autoInstall) {
      depsConfirmed = true;
    }

    // Order: deps confirmation (sauf si auto-install ON) → webhook config → install.
    if (hasPipDeps && !depsConfirmed) {
      step = "deps-confirm";
    } else if (needsConfig) {
      step = "configure";
    } else {
      handleInstall();
    }
  }

  function confirmDepsAndProceed() {
    depsConfirmed = true;
    if (needsConfig) {
      step = "configure";
    } else {
      handleInstall();
    }
  }

  async function handleInstall() {
    if (!selectedPath) return;
    step = "installing";
    installError = null;

    // Build overrides from user-entered secrets.
    const triggerConfigs: TriggerConfigOverride[] = Object.entries(webhookSecrets).map(
      ([id, secret]) => ({ id, secret: secret || undefined }),
    );

    try {
      installResult = await installPackage(selectedPath, triggerConfigs, depsConfirmed);
      step = "done";
      oninstalled(installResult!);
    } catch (e) {
      const err = String(e);
      // Si le backend exige une confirmation explicite (race / stale state /
      // auto-install désactivé entre temps), on bounce vers la nouvelle étape.
      const autoInstall = get(agentInstallPrefs).autoInstallPythonDeps;
      if (err.includes("DEPS_CONFIRMATION_REQUIRED") && !autoInstall) {
        installError = null;
        step = "deps-confirm";
      } else {
        installError = err;
        // Revenir à l'étape utile la plus proche pour permettre une nouvelle
        // tentative sans tout recommencer.
        step = needsConfig ? "configure" : "preview";
      }
    }
  }

  async function copyToClipboard(text: string, id: string) {
    await navigator.clipboard.writeText(text);
    copiedId = id;
    setTimeout(() => (copiedId = null), 1500);
  }

  function webhookEndpoint(triggerId: string) {
    return `http://localhost:7771/api/v1/webhooks/${triggerId}`;
  }

  function triggerIcon(type: string) {
    return type === "webhook" ? Webhook : type === "interval" ? Timer : type === "file_watch" ? Eye : Clock;
  }

  function triggerDetail(tr: TriggerPreview): string {
    if (tr.schedule) return tr.schedule;
    if (tr.every) return get(t)("agents.install.trigger_every", { values: { every: tr.every } });
    if (tr.path) return tr.path;
    return "";
  }

  // Webhook secrets must be ≥ 32 chars (matches validation.rs MIN_WEBHOOK_SECRET_LENGTH).
  const canInstall = $derived(
    !needsConfig ||
    webhookTriggers.every((t) => !t.needs_config || (webhookSecrets[t.id] ?? "").trim().length >= 32),
  );
</script>

<Dialog {open} onclose={handleClose} title={$t("agents.install.title")} size="md">

  <!-- ── Step: pick ──────────────────────────────────────────────────────── -->
  {#if step === "pick"}
    <div class="py-4 flex flex-col items-center gap-4 text-center">
      {#if previewLoading}
        <div class="size-16 rounded-2xl bg-primary/10 flex items-center justify-center animate-pulse">
          <Package2 size={28} class="text-primary/60" />
        </div>
        <p class="text-sm text-muted-foreground">{$t("agents.install.analyzing")}</p>
      {:else}
        <div class="size-16 rounded-2xl bg-primary/10 flex items-center justify-center">
          <FolderOpen size={28} class="text-primary" />
        </div>
        <div>
          <p class="text-sm font-medium">{$t("agents.install.pick_folder")}</p>
          <p class="text-xs text-muted-foreground mt-1">
            {$t("agents.install.pick_folder_hint_prefix")} <code class="bg-muted px-1 rounded">agent.toml</code>
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
          {$t("agents.install.choose_folder")}
        </Button>
      {/if}
    </div>

  <!-- ── Step: preview ──────────────────────────────────────────────────── -->
  {:else if step === "preview" && preview}
    <div class="py-2 space-y-4">
      <!-- Package header -->
      <div class="flex items-center gap-3">
        <div class="size-10 rounded-xl bg-primary/10 flex items-center justify-center shrink-0">
          <Package size={18} class="text-primary" />
        </div>
        <div>
          <p class="font-medium text-sm">{preview.name}</p>
          <p class="text-xs text-muted-foreground">v{preview.version} · {preview.author || "-"}</p>
        </div>
        <div class="ml-auto">
          {#if preview.valid}
            <Badge variant="success" class="text-[10px] px-2 py-0.5 gap-1">
              <CheckCircle2 size={10} />{$t("agents.install.valid")}
            </Badge>
          {:else}
            <Badge variant="destructive" class="text-[10px] px-2 py-0.5 gap-1">
              <AlertTriangle size={10} />{$t("agents.install.invalid")}
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
          <Users size={10} />{$t("agents.install.section_agents", { values: { n: preview.agents.length } })}
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

      <!-- Triggers -->
      {#if preview.triggers.length > 0}
        <div>
          <h4 class="text-[10px] font-medium uppercase text-muted-foreground tracking-wide mb-1.5 flex items-center gap-1">
            <Zap size={10} />{$t("agents.install.section_triggers", { values: { n: preview.triggers.length } })}
          </h4>
          <div class="space-y-1">
            {#each preview.triggers as trigger (trigger.id)}
              {@const Icon = triggerIcon(trigger.source_type)}
              {@const detail = triggerDetail(trigger)}
              <div class="flex items-center gap-2 rounded-lg bg-muted/30 px-2.5 py-1.5 text-xs">
                <Icon size={11} class="shrink-0 text-muted-foreground" />
                <span class="font-medium flex-1 truncate">{trigger.id}</span>
                {#if detail}
                  <span class="text-muted-foreground font-mono text-[10px] shrink-0">{detail}</span>
                {/if}
                <Badge variant="outline" class="text-[9px] px-1.5 py-0 shrink-0">{trigger.source_type}</Badge>
                {#if trigger.needs_config}
                  <Badge variant="warning" class="text-[9px] px-1.5 py-0 gap-0.5 shrink-0">
                    <Settings size={8} />config
                  </Badge>
                {/if}
              </div>
            {/each}
          </div>
          {#if needsConfig}
            <p class="mt-1.5 text-[10px] text-amber-500/80 flex items-center gap-1">
              <AlertTriangle size={10} />
              {$t("agents.install.triggers_need_config", { values: { n: webhookTriggers.filter((tr) => tr.needs_config).length } })}
            </p>
          {/if}
        </div>
      {/if}

      <!-- pip packages -->
      {#if preview.pip_packages.length > 0}
        <div>
          <h4 class="text-[10px] font-medium uppercase text-muted-foreground tracking-wide mb-1.5 flex items-center gap-1">
            <Package2 size={10} />{$t("agents.install.section_pip_deps", { values: { n: preview.pip_packages.length } })}
          </h4>
          {#if $agentInstallPrefs.autoInstallPythonDeps}
            <div class="rounded-lg border border-primary/30 bg-primary/5 px-3 py-2 text-[11px] text-primary">
              <p class="leading-relaxed">
                {preview.pip_packages.length > 1
                  ? $t("agents.install.deps_auto_many", { values: { n: preview.pip_packages.length } })
                  : $t("agents.install.deps_auto_one", { values: { n: preview.pip_packages.length } })}
              </p>
            </div>
          {:else}
            <div class="rounded-lg border border-amber-500/30 bg-amber-500/5 px-3 py-2 text-[11px] text-amber-700 dark:text-amber-300">
              <p class="leading-relaxed">
                {preview.pip_packages.length > 1
                  ? $t("agents.install.deps_manual_many", { values: { n: preview.pip_packages.length } })
                  : $t("agents.install.deps_manual_one", { values: { n: preview.pip_packages.length } })}
                <span class="font-medium">{$t("agents.install.deps_confirm_step")}</span> {$t("agents.install.deps_before_download")}
              </p>
            </div>
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
        <Button variant="outline" onclick={reset} class="flex-1">{$t("agents.install.change")}</Button>
        <Button onclick={proceedFromPreview} class="flex-1 gap-1.5" disabled={!preview.valid}>
          {willShowDepsConfirm
            ? $t("agents.install.continue")
            : needsConfig
              ? $t("agents.install.configure")
              : $t("agents.install.install")}
          {#if needsConfig && !willShowDepsConfirm}<Settings size={13} />{/if}
        </Button>
      </div>
    </div>

  <!-- ── Step: deps-confirm (pip packages confirmation) ─────────────────── -->
  {:else if step === "deps-confirm" && preview}
    <div class="py-2 space-y-4">
      <div class="flex items-center gap-2 text-sm font-medium">
        <Package2 size={15} class="text-primary" />
        {$t("agents.install.deps_confirm_title")}
      </div>

      <div class="rounded-lg border border-amber-500/40 bg-amber-500/5 px-3 py-2.5 text-xs text-amber-700 dark:text-amber-300 leading-relaxed space-y-1.5">
        <p>
          {$t("agents.install.deps_confirm_intro_prefix", { values: { name: preview.name } })}
          <span class="font-medium">{preview.pip_packages.length > 1
            ? $t("agents.install.deps_confirm_count_many", { values: { n: preview.pip_packages.length } })
            : $t("agents.install.deps_confirm_count_one", { values: { n: preview.pip_packages.length } })}</span>
          {$t("agents.install.deps_confirm_intro_suffix")} <code class="text-[10px] bg-amber-500/10 px-1 rounded font-mono">~/.apollia/venvs/</code>.
        </p>
        <p class="text-amber-600/80 dark:text-amber-400/80">
          {$t("agents.install.deps_confirm_no_install")}
        </p>
      </div>

      <!-- List of packages -->
      <div>
        <h4 class="text-[10px] font-medium uppercase text-muted-foreground tracking-wide mb-1.5">
          {$t("agents.install.deps_list_title")}
        </h4>
        <div class="rounded-lg border border-border/60 bg-muted/20 max-h-48 overflow-y-auto">
          {#each preview.pip_packages as pkg (pkg)}
            <div class="flex items-center gap-2 px-2.5 py-1.5 text-xs font-mono border-b border-border/40 last:border-0">
              <Package2 size={10} class="text-muted-foreground shrink-0" />
              <span class="truncate">{pkg}</span>
            </div>
          {/each}
        </div>
      </div>

      <p class="text-[10px] text-muted-foreground leading-relaxed">
        {$t("agents.install.deps_venv_note")}
        {$t("agents.install.deps_provenance_prefix")} <a href="https://pypi.org" class="underline">pypi.org</a>.
      </p>

      {#if installError}
        <div class="flex items-start gap-2 rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive">
          <AlertTriangle size={13} class="mt-0.5 shrink-0" />
          <span>{installError}</span>
        </div>
      {/if}

      <div class="flex gap-2 pt-1">
        <Button variant="outline" onclick={() => (step = "preview")} class="flex-1">{$t("agents.install.back")}</Button>
        <Button onclick={confirmDepsAndProceed} class="flex-1 gap-1.5">
          <CheckCircle2 size={13} />
          {needsConfig ? $t("agents.install.confirm_and_continue") : $t("agents.install.confirm_and_install")}
        </Button>
      </div>
    </div>

  <!-- ── Step: configure (webhook secrets) ─────────────────────────────── -->
  {:else if step === "configure" && preview}
    <div class="py-2 space-y-4">
      <div class="flex items-center gap-2 text-sm font-medium">
        <Settings size={15} class="text-primary" />
        {$t("agents.install.configure_title")}
      </div>
      <p class="text-xs text-muted-foreground leading-relaxed">
        {$t("agents.install.configure_intro")}
      </p>

      {#each webhookTriggers as trigger (trigger.id)}
        <div class="rounded-lg border border-border/60 p-3 space-y-2.5">
          <div class="flex items-center justify-between">
            <div class="flex items-center gap-1.5">
              <Webhook size={13} class="text-primary" />
              <span class="text-sm font-medium">{trigger.id}</span>
              {#if trigger.agent}
                <span class="text-[10px] text-muted-foreground">→ {trigger.agent}</span>
              {/if}
            </div>
            {#if !trigger.needs_config}
              <Badge variant="success" class="text-[9px] px-1.5 py-0 gap-0.5">
                <CheckCircle2 size={8} />{$t("agents.install.preconfigured")}
              </Badge>
            {/if}
          </div>

          <!-- Endpoint URL -->
          <div class="space-y-1">
            <p class="text-[10px] text-muted-foreground uppercase tracking-wide">{$t("agents.install.endpoint")}</p>
            <div class="flex items-center gap-1.5 rounded bg-muted/50 px-2 py-1.5">
              <code class="text-[10px] flex-1 truncate text-muted-foreground font-mono">
                {webhookEndpoint(trigger.id)}
              </code>
              <button
                class="shrink-0 text-muted-foreground hover:text-foreground transition-colors"
                onclick={() => copyToClipboard(webhookEndpoint(trigger.id), trigger.id + "-url")}
                title={$t("agents.install.copy_url")}
              >
                {#if copiedId === trigger.id + "-url"}
                  <CheckCircle2 size={12} class="text-success" />
                {:else}
                  <Copy size={12} />
                {/if}
              </button>
            </div>
          </div>

          <!-- Secret input -->
          <FormField
            id="secret-{trigger.id}"
            label={$t("agents.install.secret_label")}
            labelClass="text-[10px] font-normal uppercase tracking-wide"
            required={trigger.needs_config}
          >
            <div class="relative">
              <input
                id="secret-{trigger.id}"
                type="text"
                placeholder="Ex: my-super-secret-key-at-least-32-chars"
                class="w-full rounded border border-border bg-background px-2.5 py-1.5 text-xs font-mono focus:outline-none focus:ring-1 focus:ring-primary"
                bind:value={webhookSecrets[trigger.id]}
              />
              {#if (webhookSecrets[trigger.id] ?? "").trim().length > 0}
                <Button variant="ghost" size="sm"
                  class="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
                  onclick={() => copyToClipboard(webhookSecrets[trigger.id], trigger.id + "-secret")}
                  title={$t("agents.install.copy_secret")}
                >
                  {#if copiedId === trigger.id + "-secret"}
                    <CheckCircle2 size={11} class="text-success" />
                  {:else}
                    <Copy size={11} />
                  {/if}
                </Button>
              {/if}
            </div>
            {#if trigger.needs_config && (webhookSecrets[trigger.id] ?? "").trim().length > 0 && (webhookSecrets[trigger.id] ?? "").trim().length < 32}
              <p class="text-[10px] text-destructive">{$t("agents.install.secret_too_short")}</p>
            {/if}
          </FormField>
        </div>
      {/each}

      {#if installError}
        <div class="flex items-start gap-2 rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive">
          <AlertTriangle size={13} class="mt-0.5 shrink-0" />
          <span>{installError}</span>
        </div>
      {/if}

      <div class="flex gap-2 pt-1">
        <Button variant="outline" onclick={() => (step = "preview")} class="flex-1">{$t("agents.install.back")}</Button>
        <Button onclick={handleInstall} class="flex-1" disabled={!canInstall}>
          {$t("agents.install.install")}
        </Button>
      </div>
    </div>

  <!-- ── Step: installing ───────────────────────────────────────────────── -->
  {:else if step === "installing"}
    <div class="py-8 flex flex-col items-center gap-3 text-center">
      <div class="size-16 rounded-2xl bg-primary/10 flex items-center justify-center animate-pulse">
        <Package size={28} class="text-primary/60" />
      </div>
      <p class="text-sm font-medium">{$t("agents.install.installing")}</p>
      <p class="text-xs text-muted-foreground">
        {#if hasPipDeps}
          {$t("agents.install.installing_deps")}
        {:else}
          {$t("agents.install.installing_files")}
        {/if}
      </p>
    </div>

  <!-- ── Step: done ─────────────────────────────────────────────────────── -->
  {:else if step === "done" && installResult}
    <div class="py-6 flex flex-col items-center gap-4 text-center">
      <div class="size-16 rounded-2xl bg-success/10 flex items-center justify-center">
        <CheckCircle2 size={28} class="text-success" />
      </div>
      <div>
        <p class="text-sm font-medium">{$t("agents.install.done_title")}</p>
        <p class="text-xs text-muted-foreground mt-1">
          <span class="font-medium">{installResult.name}</span> v{installResult.version} ·
          {installResult.agent_count > 1
            ? $t("agents.install.done_agents_many", { values: { n: installResult.agent_count } })
            : $t("agents.install.done_agents_one", { values: { n: installResult.agent_count } })} ·
          {installResult.trigger_count > 1
            ? $t("agents.install.done_triggers_many", { values: { n: installResult.trigger_count } })
            : $t("agents.install.done_triggers_one", { values: { n: installResult.trigger_count } })}
        </p>
        {#if installResult.trigger_errors.length > 0}
          <div class="mt-3 text-left space-y-1">
            {#each installResult.trigger_errors as err}
              <div class="flex items-start gap-1.5 text-[10px] text-amber-500/80">
                <AlertTriangle size={10} class="mt-0.5 shrink-0" />
                <span>{err}</span>
              </div>
            {/each}
          </div>
        {/if}
      </div>
      <Button onclick={handleClose} class="w-full">{$t("common.close")}</Button>
    </div>
  {/if}

</Dialog>
