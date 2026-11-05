<script lang="ts">
  /**
   * EngineVariants - System > Engines: every variant of the inference engine
   * the followed channel offers for this platform, installed or not, each with
   * the verdict of the runtime on this machine and its reason. The variants
   * that cannot run here are folded behind one button. Before the channel was
   * ever read the list says so and offers the check. A variant not installed
   * is installed from its row, and its progress shows there; an incompatible
   * one asks first, with the reason. Engines copied in a folder are imported
   * from the foot of the list.
   */
  import { t } from "svelte-i18n";
  import { Download, FolderInput, Layers, RefreshCw } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import SettingsSection from "../SettingsSection.svelte";
  import EngineInstallStatus from "./EngineInstallStatus.svelte";
  import { variantLabel } from "./EngineList.svelte";
  import { installationOn, isRunning } from "$lib/settings/engineManagement";
  import { engineManagement } from "$lib/settings/engineManagementLive";
  import type { EngineVerdict, EnginesResponse } from "$lib/ipc/system";
  import {
    VERDICT_KEYS,
    incompatibleCount,
    reasonSentence,
    shownVariants,
    variantList,
    type VariantRow,
  } from "$lib/settings/engineVariants";

  interface Props {
    response: EnginesResponse | null;
    /** The system the application runs on, as `get_system_info` states it. */
    os: string | null;
    /** True while the channel is checked. */
    checking: boolean;
    oncheck: () => void;
  }

  let { response, os, checking, oncheck }: Props = $props();

  const list = $derived(variantList(response, os));
  const rows = $derived(list.kind === "rows" ? list.rows : []);
  let showIncompatible = $state(false);
  const shown = $derived(shownVariants(rows, showIncompatible));
  const hidden = $derived(incompatibleCount(rows));

  const BADGES: Record<EngineVerdict, "success" | "neutral" | "danger"> = {
    recommended: "success",
    compatible: "neutral",
    incompatible: "danger",
  };

  function name(row: VariantRow): string {
    return variantLabel(row.variant, $t("settings.engines.variant_cpu")) ?? row.variant;
  }

  function reason(row: VariantRow): string | null {
    if (!row.reason) return null;
    const sentence = reasonSentence(row.reason, row.verdict);
    return $t(sentence.key, { values: sentence.values });
  }

  const busy = $derived($engineManagement.busy || isRunning($engineManagement.installation));
  const pending = $derived($engineManagement.pendingInstall);
  const importing = $derived(installationOn($engineManagement, { kind: "import" }));

  function pendingBody(row: VariantRow): string {
    const why = reason(row);
    return why
      ? $t("settings.engines.install_confirm.body", { values: { reason: why } })
      : $t("settings.engines.install_confirm.body_no_reason");
  }
</script>

<SettingsSection
  title={$t("settings.engines.variants_title")}
  description={$t("settings.engines.variants_desc")}
  bodyLayout="flush"
  data-testid="engine-variant-list"
>
  {#snippet icon()}<Layers size={15} strokeWidth={1.75} />{/snippet}

  {#if list.kind === "unread"}
    <div class="flex flex-wrap items-center gap-3 py-3" data-testid="engine-variant-unread">
      <p class="min-w-0 flex-1 text-body-sm text-muted-foreground">
        {$t("settings.engines.variants_unread")}
      </p>
      <Button
        variant="outline"
        size="sm"
        loading={checking}
        disabled={!response?.verifiable}
        onclick={oncheck}
        data-testid="engine-variant-check"
      >
        {#snippet icon()}<RefreshCw size={13} />{/snippet}
        {$t("settings.engines.variants_check")}
      </Button>
    </div>
  {:else if list.kind === "none"}
    <p class="py-3 text-body-sm text-muted-foreground" data-testid="engine-variant-none">
      {$t("settings.engines.variants_none")}
    </p>
  {:else}
    <ul class="divide-y divide-border/50">
      {#each shown as row (row.variant)}
        <li class="flex flex-col gap-1 py-3" data-testid="engine-variant-row-{row.variant}">
          <div class="flex flex-wrap items-center gap-2">
            <span class="text-body-sm font-medium text-foreground">llama.cpp · {name(row)}</span>
            <Badge variant="neutral" outline size="sm">{row.tag}</Badge>
            {#if row.verdict}
              <Badge
                variant={BADGES[row.verdict]}
                outline={row.verdict !== "recommended"}
                size="sm"
                data-testid="engine-variant-row-{row.variant}-verdict"
              >
                {$t(VERDICT_KEYS[row.verdict])}
              </Badge>
            {/if}
            {#if row.experimental}
              <Badge variant="warning" outline size="sm" data-testid="engine-variant-row-{row.variant}-experimental">
                {$t("settings.engines.variant_experimental_windows")}
              </Badge>
            {/if}
            {#if row.installed}
              <Badge variant="info" outline size="sm" data-testid="engine-variant-row-{row.variant}-installed">
                {$t("settings.engines.variant_installed")}
              </Badge>
            {/if}
          </div>
          {#if reason(row)}
            <p
              class="text-caption {row.verdict === 'recommended' ? 'text-warning' : 'text-muted-foreground'}"
              data-testid="engine-variant-row-{row.variant}-reason"
            >
              {reason(row)}
            </p>
          {/if}
          {#if !row.installed}
            {@const running = installationOn($engineManagement, { kind: "variant", variant: row.variant })}
            {#if running}
              <EngineInstallStatus
                installation={running}
                data-testid="engine-variant-install-progress-{row.variant}"
              />
            {/if}
            <div>
              <Button
                variant={row.verdict === "recommended" ? "default" : "outline"}
                size="sm"
                disabled={busy}
                title={busy ? $t("settings.engines.install_busy") : undefined}
                aria-label={$t("settings.engines.install_label", { values: { name: name(row) } })}
                onclick={() => void engineManagement.install(row)}
                data-testid="engine-variant-install-{row.variant}"
              >
                {#snippet icon()}<Download size={13} />{/snippet}
                {$t("settings.engines.install")}
              </Button>
            </div>
          {/if}
        </li>
      {/each}
    </ul>
    {#if hidden > 0}
      <div class="pt-2">
        <Button
          variant="ghost"
          size="sm"
          onclick={() => (showIncompatible = !showIncompatible)}
          aria-expanded={showIncompatible}
          data-testid="engine-variant-toggle-incompatible"
        >
          {showIncompatible
            ? $t("settings.engines.variants_hide_incompatible")
            : $t("settings.engines.variants_show_incompatible", { values: { count: hidden } })}
        </Button>
      </div>
    {/if}
  {/if}

  <div class="flex flex-col gap-2 border-t border-border/50 py-3" data-testid="engine-import">
    <div class="flex flex-wrap items-center gap-3">
      <p class="min-w-0 flex-1 text-caption text-muted-foreground">
        {$t("settings.engines.import_hint")}
      </p>
      <Button
        variant="outline"
        size="sm"
        disabled={busy}
        title={busy ? $t("settings.engines.install_busy") : undefined}
        onclick={() => void engineManagement.importFromFolder()}
        data-testid="engine-import-folder"
      >
        {#snippet icon()}<FolderInput size={13} />{/snippet}
        {$t("settings.engines.import_folder")}
      </Button>
    </div>
    {#if importing}
      <EngineInstallStatus installation={importing} data-testid="engine-import-folder-progress" />
    {/if}
  </div>
</SettingsSection>

<ConfirmDialog
  open={pending !== null}
  onclose={() => engineManagement.cancelInstall()}
  onconfirm={() => void engineManagement.confirmInstall()}
  title={pending
    ? $t("settings.engines.install_confirm.title", { values: { name: name(pending) } })
    : ""}
  message={pending ? pendingBody(pending) : ""}
  confirmLabel={$t("settings.engines.install_confirm.confirm")}
  data-testid="engine-variant-install-dialog"
/>
