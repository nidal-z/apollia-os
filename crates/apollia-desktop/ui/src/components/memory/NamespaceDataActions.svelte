<script lang="ts">
  /**
   * Namespace-level data ownership: export to JSON, import from JSON, purge by
   * age.
   *
   * The three gestures are the operator's only way to take their memory out of
   * the machine, put it back, and shrink it. Export and import go through the
   * native file picker, so the runtime only ever receives a path the operator
   * chose. Purge is irreversible and therefore two-step: a parameter step that
   * shows how many of the listed entries match, then an explicit confirmation.
   */
  import { t } from "svelte-i18n";
  import { Download, Upload, Trash2, AlertTriangle, Loader2 } from "lucide-svelte";
  import {
    open as openFilePicker,
    save as saveFilePicker,
  } from "@tauri-apps/plugin-dialog";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Select } from "$lib/components/ui/select";
  import { FormField } from "$lib/components/ui/form-field";
  import { Dialog, DialogFooter } from "$lib/components/ui/dialog";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import { addToast } from "$lib/components/ui/toast/store";
  import { reportError } from "$lib/errors/reportError";
  import { listMemoryEntries } from "$lib/ipc/projects";
  import {
    memoryExportNamespace,
    memoryImportNamespace,
    purgeMemory,
    type MemoryImportMode,
    type MemoryPurgeScope,
  } from "$lib/ipc/memory";
  import { countPurgeMatches } from "./purgePreview";
  import type { MemoryEntry } from "$lib/types";

  interface Props {
    /** Namespace the three gestures apply to. Empty disables all of them. */
    namespace: string;
    /** Called after an import or a purge changed what is stored. */
    onchanged: () => void | Promise<void>;
  }

  let { namespace, onchanged }: Props = $props();

  const DEFAULT_PURGE_DAYS = 30;

  const disabled = $derived(namespace === "");

  // ── Export ──────────────────────────────────────────────────────────────────
  let exporting = $state(false);

  /** Namespaces may carry `:` (project scoping), which is not filename-safe. */
  function defaultExportName(): string {
    const slug = namespace.replace(/[^a-zA-Z0-9._-]+/g, "-");
    const day = new Date().toISOString().slice(0, 10);
    return `${slug}-memory-${day}.json`;
  }

  async function handleExport(): Promise<void> {
    if (exporting || disabled) return;
    exporting = true;
    try {
      const destination = await saveFilePicker({
        defaultPath: defaultExportName(),
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (destination === null) return;
      const summary = await memoryExportNamespace(namespace, destination);
      addToast(summary, "success", { "data-testid": "memory-export-done" });
    } catch (e) {
      reportError(e, { surface: "toast", testid: "memory-export-error" });
    } finally {
      exporting = false;
    }
  }

  // ── Import ──────────────────────────────────────────────────────────────────
  let importOpen = $state(false);
  let importReplaceConfirmOpen = $state(false);
  let importing = $state(false);
  let importPath = $state("");

  // The `<select>` binding is a plain string; the union is recovered here so
  // the value handed to the command can only ever be one the runtime accepts.
  let importModeValue = $state("merge");
  const importMode = $derived<MemoryImportMode>(
    importModeValue === "replace" ? "replace" : "merge",
  );

  const importFileName = $derived(
    importPath === "" ? "" : (importPath.split(/[/\\]/).pop() ?? importPath),
  );

  function openImport(): void {
    importModeValue = "merge";
    importPath = "";
    importOpen = true;
  }

  async function pickImportFile(): Promise<void> {
    try {
      const picked = await openFilePicker({
        multiple: false,
        directory: false,
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (typeof picked === "string") {
        importPath = picked;
      }
    } catch (e) {
      reportError(e, { surface: "toast", testid: "memory-import-pick-error" });
    }
  }

  function submitImport(): void {
    if (importPath === "" || importing) return;
    // `replace` wipes the namespace before inserting, so it gets its own
    // confirmation instead of running straight off the primary button.
    if (importMode === "replace") {
      importReplaceConfirmOpen = true;
      return;
    }
    void runImport();
  }

  async function runImport(): Promise<void> {
    if (importPath === "" || importing) return;
    importing = true;
    try {
      const summary = await memoryImportNamespace(
        namespace,
        importPath,
        importMode,
      );
      addToast(summary, "success", { "data-testid": "memory-import-done" });
      importReplaceConfirmOpen = false;
      importOpen = false;
      await onchanged();
    } catch (e) {
      reportError(e, { surface: "toast", testid: "memory-import-error" });
    } finally {
      importing = false;
    }
  }

  // ── Purge ───────────────────────────────────────────────────────────────────
  let purgeOpen = $state(false);
  let purgeConfirmOpen = $state(false);
  let purging = $state(false);
  let purgeScopeValue = $state("all");
  let purgeDaysInput = $state(String(DEFAULT_PURGE_DAYS));
  let purgeSnapshot = $state<MemoryEntry[] | null>(null);
  let snapshotLoading = $state(false);

  const purgeScopes: MemoryPurgeScope[] = [
    "all",
    "episodic",
    "semantic",
    "procedural",
  ];

  const scopeLabelKeys: Record<MemoryPurgeScope, string> = {
    all: "memory.data.scope.all",
    episodic: "memory.type.episodic",
    semantic: "memory.type.semantic",
    procedural: "memory.type.procedural",
  };

  function isPurgeScope(value: string): value is MemoryPurgeScope {
    return (purgeScopes as string[]).includes(value);
  }

  const purgeScope = $derived<MemoryPurgeScope>(
    isPurgeScope(purgeScopeValue) ? purgeScopeValue : "all",
  );

  /** `null` when the field does not hold a usable non-negative day count. */
  const purgeDays = $derived.by(() => {
    const trimmed = purgeDaysInput.trim();
    if (!/^\d+$/.test(trimmed)) return null;
    const parsed = Number.parseInt(trimmed, 10);
    return Number.isSafeInteger(parsed) ? parsed : null;
  });

  /**
   * How many of the listed entries the purge would take. The command exposes no
   * dry run, so this counts the snapshot the explorer can see; the runtime
   * return value is the authoritative figure and lands in the success toast.
   */
  const matchingEntries = $derived.by(() => {
    const snapshot = purgeSnapshot;
    const days = purgeDays;
    if (snapshot === null || days === null) return null;
    return countPurgeMatches(snapshot, purgeScope, days, Date.now());
  });

  const purgeReady = $derived(purgeDays !== null && !purging);

  async function openPurge(): Promise<void> {
    purgeScopeValue = "all";
    purgeDaysInput = String(DEFAULT_PURGE_DAYS);
    purgeSnapshot = null;
    purgeOpen = true;
    snapshotLoading = true;
    try {
      purgeSnapshot = await listMemoryEntries(namespace);
    } catch {
      // No snapshot means no preview; the purge itself stays available and the
      // returned count tells the operator what left.
      purgeSnapshot = null;
    } finally {
      snapshotLoading = false;
    }
  }

  function askPurgeConfirmation(): void {
    if (!purgeReady) return;
    purgeConfirmOpen = true;
  }

  async function runPurge(): Promise<void> {
    const days = purgeDays;
    if (days === null || purging) return;
    purging = true;
    try {
      const removed = await purgeMemory(namespace, days, purgeScope);
      addToast(
        $t("memory.data.purge.done", { values: { count: removed } }),
        "success",
        { "data-testid": "memory-purge-done" },
      );
      purgeConfirmOpen = false;
      purgeOpen = false;
      await onchanged();
    } catch (e) {
      reportError(e, { surface: "toast", testid: "memory-purge-error" });
    } finally {
      purging = false;
    }
  }

  const purgeConfirmMessage = $derived(
    $t("memory.data.purge.confirm_message", {
      values: {
        days: purgeDays ?? 0,
        scope: $t(scopeLabelKeys[purgeScope]).toLowerCase(),
        namespace,
      },
    }),
  );
</script>

<div class="flex items-center gap-1.5" data-testid="memory-data-actions">
  <Button
    variant="outline"
    size="sm"
    type="button"
    disabled={disabled || exporting}
    onclick={handleExport}
    data-testid="memory-export-button"
  >
    {#if exporting}
      <Loader2 size={13} class="mr-1.5 animate-spin" aria-hidden="true" />
    {:else}
      <Download size={13} class="mr-1.5" aria-hidden="true" />
    {/if}
    {$t("memory.data.export.button")}
  </Button>

  <Button
    variant="outline"
    size="sm"
    type="button"
    {disabled}
    onclick={openImport}
    data-testid="memory-import-button"
  >
    <Upload size={13} class="mr-1.5" aria-hidden="true" />
    {$t("memory.data.import.button")}
  </Button>

  <Button
    variant="outline"
    size="sm"
    type="button"
    {disabled}
    onclick={openPurge}
    class="border-destructive/40 text-destructive hover:bg-destructive/10"
    data-testid="memory-purge-button"
  >
    <Trash2 size={13} class="mr-1.5" aria-hidden="true" />
    {$t("memory.data.purge.button")}
  </Button>
</div>

<!-- Import: source file + merge/replace strategy -->
<Dialog
  open={importOpen}
  onclose={() => (importOpen = false)}
  size="md"
  title={$t("memory.data.import.title")}
  data-testid="memory-import-dialog"
>
  <p class="text-caption text-muted-foreground">
    {$t("memory.data.import.description", { values: { namespace } })}
  </p>

  <div class="mt-4 space-y-4">
    <FormField
      label={$t("memory.data.import.file_label")}
      id="memory-import-file"
      hint={$t("memory.data.import.file_hint")}
    >
      <div class="flex items-center gap-2">
        <Button
          variant="outline"
          size="sm"
          type="button"
          onclick={pickImportFile}
          data-testid="memory-import-pick"
        >
          {$t("memory.data.import.pick_file")}
        </Button>
        <span
          class="min-w-0 flex-1 truncate text-caption text-muted-foreground"
          data-testid="memory-import-path"
          title={importPath}
        >
          {importFileName === ""
            ? $t("memory.data.import.no_file")
            : importFileName}
        </span>
      </div>
    </FormField>

    <FormField
      label={$t("memory.data.import.mode_label")}
      id="memory-import-mode"
      hint={importMode === "replace"
        ? $t("memory.data.import.mode_replace_hint")
        : $t("memory.data.import.mode_merge_hint")}
    >
      <Select
        id="memory-import-mode"
        size="sm"
        bind:value={importModeValue}
        data-testid="memory-import-mode"
      >
        <option value="merge">{$t("memory.data.import.mode_merge")}</option>
        <option value="replace">{$t("memory.data.import.mode_replace")}</option>
      </Select>
    </FormField>

    {#if importMode === "replace"}
      <div
        class="flex items-start gap-2 rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2"
        data-testid="memory-import-replace-warning"
      >
        <AlertTriangle
          size={14}
          class="mt-0.5 shrink-0 text-destructive"
          aria-hidden="true"
        />
        <p class="text-caption text-muted-foreground">
          {$t("memory.data.import.replace_warning")}
        </p>
      </div>
    {/if}
  </div>

  <DialogFooter>
    <Button
      variant="outline"
      type="button"
      onclick={() => (importOpen = false)}
      data-testid="memory-import-cancel"
    >
      {$t("common.cancel")}
    </Button>
    <Button
      type="button"
      disabled={importPath === "" || importing}
      onclick={submitImport}
      data-testid="memory-import-submit"
    >
      {#if importing}
        <Loader2 size={14} class="mr-2 animate-spin" aria-hidden="true" />
      {/if}
      {$t("memory.data.import.submit")}
    </Button>
  </DialogFooter>
</Dialog>

<ConfirmDialog
  open={importReplaceConfirmOpen}
  onclose={() => (importReplaceConfirmOpen = false)}
  onconfirm={runImport}
  title={$t("memory.data.import.replace_confirm_title")}
  message={$t("memory.data.import.replace_confirm_message", {
    values: { namespace },
  })}
  confirmLabel={$t("memory.data.import.replace_confirm_yes")}
  cancelLabel={$t("common.cancel")}
  loading={importing}
  data-testid="memory-import-replace-confirm"
/>

<!-- Purge step 1: scope, age, and how many listed entries match -->
<Dialog
  open={purgeOpen}
  onclose={() => (purgeOpen = false)}
  size="md"
  title={$t("memory.data.purge.title")}
  data-testid="memory-purge-dialog"
>
  <p class="text-caption text-muted-foreground">
    {$t("memory.data.purge.description", { values: { namespace } })}
  </p>

  <div class="mt-4 space-y-4">
    <FormField
      label={$t("memory.data.purge.scope_label")}
      id="memory-purge-scope"
    >
      <Select
        id="memory-purge-scope"
        size="sm"
        bind:value={purgeScopeValue}
        data-testid="memory-purge-scope"
      >
        {#each purgeScopes as scope (scope)}
          <option value={scope}>{$t(scopeLabelKeys[scope])}</option>
        {/each}
      </Select>
    </FormField>

    <FormField
      label={$t("memory.data.purge.days_label")}
      id="memory-purge-days"
      hint={$t("memory.data.purge.days_hint")}
      error={purgeDays === null ? $t("memory.data.purge.days_error") : undefined}
    >
      <!-- `type="text"` on purpose: `type="number"` makes Svelte coerce the
           binding to `number | undefined`, which loses the "field is being
           edited and currently invalid" state this dialog needs. -->
      <Input
        id="memory-purge-days"
        size="sm"
        type="text"
        inputmode="numeric"
        bind:value={purgeDaysInput}
        data-testid="memory-purge-days"
      />
    </FormField>

    <div
      class="rounded-md border border-border bg-muted/40 px-3 py-2"
      data-testid="memory-purge-preview"
    >
      {#if snapshotLoading}
        <p class="text-caption text-muted-foreground">
          {$t("memory.data.purge.preview_loading")}
        </p>
      {:else if matchingEntries === null}
        <p class="text-caption text-muted-foreground">
          {$t("memory.data.purge.preview_unavailable")}
        </p>
      {:else}
        <p class="text-body-xs text-foreground">
          {$t("memory.data.purge.preview", {
            values: { count: matchingEntries },
          })}
        </p>
        <p class="mt-0.5 text-caption text-muted-foreground">
          {$t("memory.data.purge.preview_note")}
        </p>
      {/if}
    </div>
  </div>

  <DialogFooter>
    <Button
      variant="outline"
      type="button"
      onclick={() => (purgeOpen = false)}
      data-testid="memory-purge-cancel"
    >
      {$t("common.cancel")}
    </Button>
    <Button
      variant="destructive"
      type="button"
      disabled={!purgeReady}
      onclick={askPurgeConfirmation}
      data-testid="memory-purge-continue"
    >
      {$t("memory.data.purge.continue")}
    </Button>
  </DialogFooter>
</Dialog>

<!-- Purge step 2: the irreversible confirmation -->
<ConfirmDialog
  open={purgeConfirmOpen}
  onclose={() => (purgeConfirmOpen = false)}
  onconfirm={runPurge}
  title={$t("memory.data.purge.confirm_title")}
  message={purgeConfirmMessage}
  confirmLabel={$t("memory.data.purge.confirm_yes")}
  cancelLabel={$t("common.cancel")}
  loading={purging}
  data-testid="memory-purge-confirm"
/>
