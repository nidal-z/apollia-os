<script lang="ts" context="module">
  /**
   * Settings > Observability - the observability *policy* sub-page.
   *
   * Distinct from the top-level Observability dashboard: the dashboard shows
   * what has already been collected (timeline, LLM costs, audit trail); this
   * page governs what will be collected (capture flags, privacy, truncation,
   * retention). It maps to the `[observability]` section of `apollia.toml`
   * (`ObservabilityConfig`, apollia-core).
   *
   * Editable via `get_observability_config` / `set_observability_config`. The
   * runtime does not hot-reload this section: persisted changes apply on the
   * next app restart, hence the persistent restart banner.
   */
  export const meta = {
    title: "settings.nav.observability",
    icon: "activity",
    group: "settings.nav.cluster_system",
    cluster: "system",
  } as const;
</script>

<script lang="ts">
  import { onMount } from "svelte";
  import { t, locale } from "svelte-i18n";
  import {
    Eye,
    Lock,
    Scissors,
    Clock,
    ExternalLink,
    ArrowRight,
  } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Toggle } from "$lib/components/ui/toggle";
  import { ErrorBanner } from "$lib/components/operator";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import { reportError } from "$lib/errors/reportError";
  import type { HumanizedError } from "$lib/errors/humanize";
  import { navigateTo } from "$lib/stores/navigation";
  import { openConfigInEditor } from "$lib/ipc/app";
  import {
    getObservabilityConfig,
    setObservabilityConfig,
    type ObservabilityConfig,
  } from "$lib/ipc/observability";
  import { useSettingsForm } from "$lib/settings/useSettingsForm";
  import SettingSectionSkeleton from "../../components/settings/SettingSectionSkeleton.svelte";
  import SettingsSubPage from "../../components/settings/SettingsSubPage.svelte";
  import SettingsSection from "../../components/settings/SettingsSection.svelte";
  import SettingsFieldRow from "../../components/settings/SettingsFieldRow.svelte";
  import SensitiveBadge from "../../components/settings/observability/SensitiveBadge.svelte";

  type BoolKey =
    | "capture_thoughts"
    | "capture_tool_args"
    | "capture_tool_outputs"
    | "capture_agent_logs"
    | "debug_log_prompt";
  type ByteKey = "max_input_bytes" | "max_output_bytes" | "max_tool_output_bytes";

  const CAPTURE_FLAGS: ReadonlyArray<{ key: BoolKey; sensitive: boolean }> = [
    { key: "capture_thoughts", sensitive: false },
    { key: "capture_tool_args", sensitive: false },
    { key: "capture_tool_outputs", sensitive: false },
    { key: "capture_agent_logs", sensitive: false },
  ];
  const TRUNCATION_BYTES: ReadonlyArray<ByteKey> = [
    "max_input_bytes",
    "max_output_bytes",
    "max_tool_output_bytes",
  ];

  // `debug_log_prompt` is the only setting that can expose prompt content: it
  // writes the full prompt to the TRACE log stream. It keeps the badge and the
  // informed-consent dialog. `capture_llm_prompts` used to sit here too, and it
  // never captured anything, so it warned about a risk that did not exist.
  function isSensitive(key: BoolKey): boolean {
    return key === "debug_log_prompt";
  }

  let cfg = $state<ObservabilityConfig | null>(null);
  let savedBaseline = $state<ObservabilityConfig | null>(null);
  let loading = $state(true);
  let loadError = $state<HumanizedError | null>(null);

  type ObsForm = ReturnType<typeof useSettingsForm<ObservabilityConfig>>;
  let form: ObsForm | null = null;

  // Bumped to force a sensitive Toggle to re-render back to its real (off) state
  // after the user dismisses the informed-consent dialog.
  let sensitiveNonce = $state(0);
  let confirmOpen = $state(false);
  let pendingKey = $state<BoolKey | null>(null);

  let openingEditor = $state(false);

  async function load(): Promise<void> {
    loading = true;
    loadError = null;
    try {
      const loaded = await getObservabilityConfig();
      cfg = { ...loaded };
      savedBaseline = { ...loaded };
    } catch (err) {
      loadError = reportError(err, {
        surface: "inline",
        testid: "observability-load-error",
      });
    } finally {
      loading = false;
    }
  }

  // The form registers an onDestroy hook, so it can only be built while the
  // component context is live. Creating it inside load(), past the await, threw
  // and turned every visit to this sub-page into a load error.
  $effect(() => {
    if (!cfg || form) return;
    form = useSettingsForm<ObservabilityConfig>({
      route: "observability",
      initial: $state.snapshot(cfg) as ObservabilityConfig,
      autoSave: "explicit",
      onSave: async (values) => {
        try {
          await setObservabilityConfig(values);
        } catch (err) {
          // Humanize so the frame's save toast shows friendly copy.
          const h = reportError(err, { surface: "inline" });
          throw new Error(h.friendly_message);
        }
      },
    });
  });

  $effect(() => {
    if (cfg && form) {
      form.sync($state.snapshot(cfg) as ObservabilityConfig);
    }
  });

  async function handleSave(): Promise<boolean> {
    if (!form) return false;
    const ok = await form.save();
    if (ok && cfg) {
      savedBaseline = { ...($state.snapshot(cfg) as ObservabilityConfig) };
    }
    return ok;
  }

  function handleReset(): void {
    form?.reset();
    if (savedBaseline) cfg = { ...savedBaseline };
  }

  function requestToggle(key: BoolKey, next: boolean): void {
    if (!cfg) return;
    if (next && isSensitive(key)) {
      pendingKey = key;
      confirmOpen = true;
      // Snap the optimistic Toggle flip back to off; real value on confirm.
      sensitiveNonce += 1;
      return;
    }
    cfg[key] = next;
  }

  function confirmSensitive(): void {
    if (cfg && pendingKey) cfg[pendingKey] = true;
    confirmOpen = false;
    pendingKey = null;
  }

  function cancelSensitive(): void {
    confirmOpen = false;
    pendingKey = null;
    sensitiveNonce += 1;
  }

  function setByte(key: ByteKey, raw: string): void {
    if (!cfg) return;
    const n = Number.parseInt(raw, 10);
    cfg[key] = Number.isFinite(n) && n >= 0 ? n : 0;
  }

  function setRetention(raw: string): void {
    if (!cfg) return;
    const n = Number.parseInt(raw, 10);
    cfg.retention_days = Number.isFinite(n) && n >= 1 ? n : 1;
  }

  function toKiB(bytes: number): string {
    return (bytes / 1024).toLocaleString($locale ?? "en", {
      maximumFractionDigits: 1,
    });
  }

  async function openEditor(): Promise<void> {
    openingEditor = true;
    try {
      await openConfigInEditor();
    } catch (err) {
      reportError(err, {
        surface: "toast",
        testid: "observability-open-editor-error-toast",
      });
    } finally {
      openingEditor = false;
    }
  }

  function openDashboard(): void {
    navigateTo("observability");
  }

  onMount(() => {
    void load();
  });
</script>

{#snippet toggleRow(key: BoolKey, checked: boolean, sensitive: boolean)}
  <SettingsFieldRow
    label={$t(`settings.observability.${key}`)}
    hint={$t(`settings.observability.${key}_desc`)}
  >
    {#snippet control()}
      {#if sensitive}
        <SensitiveBadge label={$t("settings.observability.sensitive_badge")} />
      {/if}
      {#key `${key}-${checked}-${sensitiveNonce}`}
        <Toggle
          {checked}
          onchange={(v) => requestToggle(key, v)}
          aria-label={$t(`settings.observability.${key}`)}
          data-testid={`observability-toggle-${key}`}
        />
      {/key}
    {/snippet}
  </SettingsFieldRow>
{/snippet}

{#snippet byteRow(key: ByteKey, value: number)}
  <SettingsFieldRow
    label={$t(`settings.observability.${key}`)}
    hint={$t(`settings.observability.${key}_desc`)}
    for={`observability-${key}`}
  >
    {#snippet control()}
      <div class="flex items-center gap-2.5">
        <div class="relative w-40">
          <Input
            id={`observability-${key}`}
            type="number"
            min="0"
            step="1024"
            {value}
            oninput={(e) => setByte(key, e.currentTarget.value)}
            class="pr-14"
            data-testid={`observability-input-${key}`}
          />
          <span
            class="pointer-events-none absolute inset-y-0 right-3 flex items-center text-caption text-muted-foreground"
          >
            {$t("settings.observability.unit_bytes")}
          </span>
        </div>
        <span class="tabular-nums text-caption text-muted-foreground">
          {$t("settings.observability.kib_display", {
            values: { kib: toKiB(value) },
          })}
        </span>
      </div>
    {/snippet}
  </SettingsFieldRow>
{/snippet}

{#if loading}
  <SettingSectionSkeleton rows={3} />
{:else if loadError}
  <ErrorBanner
    message={loadError.friendly_message}
    data-testid="observability-load-error"
  />
{:else if cfg}
  {@const c = cfg}
  <SettingsSubPage
    route="observability"
    onSave={handleSave}
    onReset={handleReset}
    data-testid="observability-settings"
  >
    <div class="flex flex-wrap items-start justify-between gap-3">
      <p class="max-w-prose text-body-sm text-muted-foreground">
        {$t("settings.observability.editable_note")}
      </p>
      <Button
        onclick={openEditor}
        disabled={openingEditor}
        variant="outline"
        size="sm"
        data-testid="observability-open-editor"
      >
        {#snippet icon()}<ExternalLink size={13} />{/snippet}
        {openingEditor ? $t("settings.opening") : $t("settings.open_editor")}
      </Button>
    </div>

    <ErrorBanner tone="warning" data-testid="observability-restart-banner">
      {$t("settings.observability.restart_note")}
    </ErrorBanner>

    <!-- Capture policy -->
    <SettingsSection
      title={$t("settings.observability.capture_section_title")}
      description={$t("settings.observability.capture_section_desc")}
      bodyLayout="flush"
      data-testid="observability-capture-section"
    >
      {#snippet icon()}<Eye size={15} strokeWidth={1.75} />{/snippet}
      {#each CAPTURE_FLAGS as flag (flag.key)}
        {@render toggleRow(flag.key, c[flag.key], flag.sensitive)}
      {/each}
    </SettingsSection>

    <!-- Privacy -->
    <SettingsSection
      title={$t("settings.observability.privacy_section_title")}
      description={$t("settings.observability.privacy_section_desc")}
      bodyLayout="flush"
      data-testid="observability-privacy-section"
    >
      {#snippet icon()}<Lock size={15} strokeWidth={1.75} />{/snippet}
      {@render toggleRow("debug_log_prompt", c.debug_log_prompt, true)}
    </SettingsSection>

    <!-- Truncation -->
    <SettingsSection
      title={$t("settings.observability.truncation_section_title")}
      description={$t("settings.observability.truncation_section_desc")}
      bodyLayout="flush"
      data-testid="observability-truncation-section"
    >
      {#snippet icon()}<Scissors size={15} strokeWidth={1.75} />{/snippet}
      {#each TRUNCATION_BYTES as key (key)}
        {@render byteRow(key, c[key])}
      {/each}
    </SettingsSection>

    <!-- Retention -->
    <SettingsSection
      title={$t("settings.observability.retention_section_title")}
      description={$t("settings.observability.retention_section_desc")}
      bodyLayout="flush"
      data-testid="observability-retention-section"
    >
      {#snippet icon()}<Clock size={15} strokeWidth={1.75} />{/snippet}

      <SettingsFieldRow
        label={$t("settings.observability.retention_days")}
        hint={$t("settings.observability.retention_days_desc")}
        for="observability-retention_days"
      >
        {#snippet control()}
          <div class="relative w-40">
            <Input
              id="observability-retention_days"
              type="number"
              min="1"
              step="1"
              value={c.retention_days}
              oninput={(e) => setRetention(e.currentTarget.value)}
              class="pr-12"
              data-testid="observability-input-retention_days"
            />
            <span
              class="pointer-events-none absolute inset-y-0 right-3 flex items-center text-caption text-muted-foreground"
            >
              {$t("settings.observability.unit_days")}
            </span>
          </div>
        {/snippet}
      </SettingsFieldRow>

      <SettingsFieldRow
        label={$t("settings.observability.view_data_label")}
        hint={$t("settings.observability.view_data_desc")}
        border={false}
      >
        {#snippet control()}
          <Button
            onclick={openDashboard}
            variant="link"
            size="sm"
            data-testid="observability-open-dashboard"
          >
            <span class="inline-flex items-center gap-1.5">
              {$t("settings.observability.view_data_link")}
              <ArrowRight size={14} aria-hidden="true" />
            </span>
          </Button>
        {/snippet}
      </SettingsFieldRow>
    </SettingsSection>
  </SettingsSubPage>
{/if}

<ConfirmDialog
  open={confirmOpen}
  onclose={cancelSensitive}
  onconfirm={confirmSensitive}
  title={$t("settings.observability.sensitive_confirm_title")}
  message={$t("settings.observability.sensitive_confirm_message")}
  confirmLabel={$t("settings.observability.sensitive_confirm_enable")}
  cancelLabel={$t("common.cancel")}
  data-testid="observability-sensitive-confirm"
/>
