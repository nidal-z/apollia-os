<script lang="ts">
  /**
   * ExportAgent - the "Export" button of an agent's header and its dialog.
   *
   * Shown for an agent created in the interface only: an agent installed from
   * a package is shared by sharing that package. The dialog says what the
   * package carries and what never enters it, offers the signature, the file
   * name and the publisher name. The name is kept in the `author.name`
   * setting, so the next export proposes it again. The file is chosen in the
   * save dialog of the system; the runtime builds and signs the archive, the
   * shell writes it. Once written, the dialog shows the key that signed, the
   * connections the package requires and the accesses it left out.
   */
  import { t } from "svelte-i18n";
  import { save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { KeyRound, PackageOpen } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { Dialog, DialogFooter } from "$lib/components/ui/dialog";
  import { Input } from "$lib/components/ui/input";
  import { addToast } from "$lib/components/ui/toast/store";
  import { exportAgent, type AgentExportReport } from "$lib/ipc/agentExport";
  import { getAgentDefinition, type ResolvedAgent } from "$lib/ipc/agentSheet";
  import { getResolvedSetting, setSetting } from "$lib/ipc/settings";
  import {
    AUTHOR_NAME_KEY,
    canExport,
    exportFileName,
    publisherOf,
    shortFingerprint,
  } from "./agentExport";

  interface Props {
    /** Identifier of the agent. */
    name: string;
    disabled?: boolean;
  }

  let { name, disabled = false }: Props = $props();

  let agent = $state<ResolvedAgent | null>(null);
  let open = $state(false);
  let sign = $state(true);
  let publisher = $state("");
  let savedPublisher = $state("");
  let exporting = $state(false);
  let error = $state<string | null>(null);
  let report = $state<AgentExportReport | null>(null);

  $effect(() => {
    const id = name;
    agent = null;
    let live = true;
    void getAgentDefinition(id)
      .then((found) => {
        if (live) agent = found;
      })
      .catch(() => {
        if (live) agent = null;
      });
    return () => {
      live = false;
    };
  });

  const fileName = $derived(agent ? exportFileName(agent) : "");

  async function openDialog(): Promise<void> {
    error = null;
    report = null;
    sign = true;
    open = true;
    try {
      const setting = await getResolvedSetting(AUTHOR_NAME_KEY);
      savedPublisher = typeof setting.value === "string" ? setting.value : "";
    } catch {
      savedPublisher = "";
    }
    publisher = savedPublisher;
  }

  function message(err: unknown): string {
    return err instanceof Error ? err.message : String(err);
  }

  async function run(): Promise<void> {
    const chosen = publisherOf(publisher);
    if (chosen === null) {
      error = $t("agent_export.publisher_required");
      return;
    }
    let path: string | null;
    try {
      path = await saveDialog({
        defaultPath: fileName,
        filters: [{ name: $t("agent_export.file_filter"), extensions: ["apo"] }],
      });
    } catch (err) {
      error = message(err);
      return;
    }
    if (!path) return;
    exporting = true;
    error = null;
    try {
      if (chosen !== savedPublisher) {
        await setSetting(AUTHOR_NAME_KEY, chosen).catch(() => undefined);
        savedPublisher = chosen;
      }
      report = await exportAgent(name, sign, chosen, path);
      addToast($t("agent_export.done"), "success");
    } catch (err) {
      error = message(err);
    } finally {
      exporting = false;
    }
  }
</script>

{#if canExport(agent)}
  <Button
    variant="outline"
    size="sm"
    {disabled}
    onclick={() => void openDialog()}
    data-testid="agent-export-btn"
  >
    {#snippet icon()}<PackageOpen size={12} />{/snippet}
    {$t("agent_export.action")}
  </Button>
  <Dialog
    {open}
    onclose={() => (open = false)}
    size="md"
    title={$t("agent_export.title", { values: { name: agent?.name ?? name } })}
    data-testid="agent-export-dialog"
  >
    {#if report}
      <div class="space-y-3" data-testid="agent-export-result">
        <p class="m-0 text-body-xs text-foreground">
          {$t("agent_export.written", { values: { path: report.path } })}
        </p>
        <p class="m-0 text-caption text-muted-foreground" data-testid="agent-export-fingerprint">
          {report.signed
            ? $t("agent_export.signed_by", {
                values: { publisher: report.publisher, fingerprint: shortFingerprint(report.key_fingerprint) },
              })
            : $t("agent_export.unsigned", { values: { publisher: report.publisher } })}
        </p>
        {#if report.required_connections.length > 0}
          <p class="m-0 text-caption text-muted-foreground" data-testid="agent-export-required">
            {$t("agent_export.required", { values: { list: report.required_connections.join(", ") } })}
          </p>
        {/if}
        {#if report.removed_access.length > 0}
          <p class="m-0 text-caption text-warning-a11y" data-testid="agent-export-removed">
            {$t("agent_export.removed", { values: { list: report.removed_access.join(", ") } })}
          </p>
        {/if}
      </div>
      <DialogFooter>
        <Button variant="primary-solid" onclick={() => (open = false)} data-testid="agent-export-close">
          {$t("common.close")}
        </Button>
      </DialogFooter>
    {:else}
      <p class="m-0 mb-4 text-caption text-muted-foreground">{$t("agent_export.legend")}</p>
      <div class="mb-4 space-y-1.5" data-testid="agent-export-never">
        <div class="text-body-xs font-semibold text-foreground">{$t("agent_export.never_title")}</div>
        {#each ["agent_export.never_secrets", "agent_export.never_memory", "agent_export.never_history"] as key (key)}
          <div class="flex items-center gap-2 text-body-xs text-foreground">
            <KeyRound size={12} class="text-muted-foreground" />
            {$t(key)}
          </div>
        {/each}
      </div>
      <label class="mb-4 flex cursor-pointer select-none items-start gap-2">
        <Checkbox bind:checked={sign} data-testid="agent-export-sign" />
        <span>
          <span class="block text-body-xs font-medium text-foreground">{$t("agent_export.sign")}</span>
          <span class="block text-caption text-muted-foreground">{$t("agent_export.sign_hint")}</span>
        </span>
      </label>
      <div class="mb-4 space-y-1">
        <div class="text-body-xs font-medium text-foreground">{$t("agent_export.file_name")}</div>
        <Input value={fileName} readonly aria-label={$t("agent_export.file_name")} data-testid="agent-export-file" />
        <p class="m-0 text-caption text-muted-foreground">{$t("agent_export.file_hint")}</p>
      </div>
      <div class="space-y-1">
        <div class="text-body-xs font-medium text-foreground">{$t("agent_export.publisher")}</div>
        <Input
          bind:value={publisher}
          placeholder={$t("agent_export.publisher_placeholder")}
          aria-label={$t("agent_export.publisher")}
          data-testid="agent-export-publisher"
        />
        <p class="m-0 text-caption text-muted-foreground">{$t("agent_export.publisher_hint")}</p>
      </div>
      {#if error}
        <p class="m-0 mt-3 text-caption text-destructive" data-testid="agent-export-error">{error}</p>
      {/if}
      <DialogFooter>
        <Button variant="outline" onclick={() => (open = false)} data-testid="agent-export-cancel">
          {$t("common.cancel")}
        </Button>
        <Button
          variant="primary-solid"
          disabled={exporting}
          onclick={() => void run()}
          data-testid="agent-export-confirm"
        >
          {exporting ? $t("agent_export.exporting") : $t("agent_export.confirm")}
        </Button>
      </DialogFooter>
    {/if}
  </Dialog>
{/if}
