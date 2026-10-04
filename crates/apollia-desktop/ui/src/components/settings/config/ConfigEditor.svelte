<script lang="ts">
  /**
   * The editor of `apollia.toml`, in Settings > Configuration: the path and its
   * actions, the notice of an event, the colored text with the marks of its
   * lines, and a footer that says the state and the caret.
   *
   * Every decision lives in `$lib/settings/configEditor` (the state, when
   * "Save" is allowed, what a change made elsewhere does); this component
   * draws it and forwards the gestures.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { Copy, FolderOpen, RotateCcw, Save } from "lucide-svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { Button } from "$lib/components/ui/button";
  import { ErrorBanner } from "$lib/components/operator";
  import { addToast } from "$lib/components/ui/toast";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import SettingSectionSkeleton from "../SettingSectionSkeleton.svelte";
  import ConfigCodeArea, { type LineMarks } from "./ConfigCodeArea.svelte";
  import ConfigDiffDialog from "./ConfigDiffDialog.svelte";
  import ConfigEditorNotice from "./ConfigEditorNotice.svelte";
  import {
    canSave,
    createConfigEditor,
    diagnosticsByLine,
    isDirty,
    lockedKeyNames,
    pendingLines,
  } from "$lib/settings/configEditor";
  import { changedLines, lockedLines } from "$lib/settings/configText";
  import {
    defaultConfigText,
    readConfigFile,
    restartApollia,
    saveConfigText,
    validateConfigText,
  } from "$lib/ipc/configFile";
  import { consumeFocusLine } from "$lib/stores/configInvalid";
  import { tasksRunningCount } from "$lib/stores/tasks";
  import { loadLineTokenizer, type CodeToken } from "$lib/utils/shikiHydrator";
  import { revealPath, isRevealablePath } from "$lib/utils/fileRef";

  const editor = createConfigEditor({
    read: readConfigFile,
    validate: validateConfigText,
    save: saveConfigText,
    defaults: defaultConfigText,
    restart: restartApollia,
  });

  let tokenize = $state<((source: string) => CodeToken[][]) | null>(null);
  let caret = $state({ line: 1, column: 1 });
  let focusLine = $state<number | null>(null);

  const tokens = $derived(tokenize ? tokenize($editor.buffer) : null);
  const marks = $derived.by(() => {
    const s = $editor;
    const byLine = new Map<number, LineMarks>();
    const at = (line: number): LineMarks => {
      let mark = byLine.get(line);
      if (!mark) byLine.set(line, (mark = {}));
      return mark;
    };
    for (const [line, message] of diagnosticsByLine(s)) at(line).error = message;
    for (const line of lockedLines(s.buffer, lockedKeyNames(s))) at(line).locked = true;
    for (const line of pendingLines(s)) at(line).restart = true;
    for (const line of changedLines(s.baseText, s.buffer)) at(line).modified = true;
    return byLine;
  });
  const saveAllowed = $derived(canSave($editor));
  const dirty = $derived(isDirty($editor));
  const revealable = $derived(!!$editor.path && isRevealablePath($editor.path));

  async function copyPath(): Promise<void> {
    try {
      await navigator.clipboard.writeText($editor.path);
      addToast($t("settings.config.path_copied"), "success");
    } catch {
      addToast($t("settings.config.copy_failed"), "error");
    }
  }

  onMount(() => {
    let disposed = false;
    let unlisten: UnlistenFn | null = null;
    void editor.load().then(() => {
      if (!disposed) focusLine = consumeFocusLine();
    });
    void loadLineTokenizer("toml").then((fn) => {
      if (!disposed) tokenize = fn;
    });
    // A file saved here, changed in another screen or in another editor ends
    // with one of these two events: re-read the file and react.
    void listen<{ event_type: string }>("runtime-event", (event) => {
      const kind = event.payload.event_type;
      if (kind === "ConfigReloaded" || kind === "ConfigInvalid") void editor.fileChanged();
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
      editor.dispose();
    };
  });
</script>

{#if $editor.status === "loading"}
  <SettingSectionSkeleton />
{:else if $editor.status === "error"}
  <ErrorBanner
    tone="danger"
    onretry={() => void editor.load()}
    retryLabel={$t("common.retry")}
    data-testid="config-editor-load-error"
  >
    <p class="font-medium">{$t("settings.config_editor.load_failed")}</p>
    <p class="text-caption opacity-90">{$editor.loadError}</p>
  </ErrorBanner>
{:else}
  <div class="flex flex-col gap-3" data-testid="config-editor">
    <p class="max-w-prose text-body-sm text-muted-foreground">
      {$t("settings.config_editor.intro")}
    </p>

    <div class="flex flex-wrap items-center justify-between gap-2">
      <code
        class="min-w-0 flex-1 truncate font-mono text-code-sm text-muted-foreground"
        title={$editor.path}
        data-testid="config-editor-path">{$editor.path}</code
      >
      <div class="flex flex-none flex-wrap items-center gap-1">
        <Button variant="ghost" size="sm" onclick={copyPath} data-testid="config-editor-copy-path">
          {#snippet icon()}<Copy size={13} />{/snippet}
          {$t("settings.config.copy_path")}
        </Button>
        {#if revealable}
          <Button
            variant="ghost"
            size="sm"
            onclick={() => void revealPath($editor.path)}
            data-testid="config-editor-reveal"
          >
            {#snippet icon()}<FolderOpen size={13} />{/snippet}
            {$t("settings.config.reveal_in_finder")}
          </Button>
        {/if}
        <Button
          variant="outline"
          size="sm"
          onclick={() => void editor.askDefaults()}
          disabled={$editor.saving}
          data-testid="config-editor-defaults"
        >
          {#snippet icon()}<RotateCcw size={13} />{/snippet}
          {$t("settings.config_editor.defaults")}
        </Button>
        <Button
          size="sm"
          onclick={() => void editor.save()}
          disabled={!saveAllowed}
          data-testid="config-editor-save"
        >
          {#snippet icon()}<Save size={13} />{/snippet}
          {$editor.saving ? $t("settings.config_editor.saving") : $t("settings.config_editor.save")}
        </Button>
      </div>
    </div>

    {#if $editor.notice}
      <ConfigEditorNotice
        notice={$editor.notice}
        onrestart={editor.askRestart}
        oncompare={editor.compareWithDisk}
        onreload={editor.reloadFromDisk}
        ondismiss={editor.dismissNotice}
      />
    {/if}

    <ConfigCodeArea
      text={$editor.buffer}
      {tokens}
      {marks}
      {focusLine}
      onedit={editor.edit}
      oncaret={(position) => (caret = position)}
    />

    <div
      class="flex flex-wrap items-center justify-between gap-2 text-caption text-muted-foreground"
      data-testid="config-editor-footer"
    >
      <span data-testid="config-editor-state">
        {#if $editor.validating}
          {$t("settings.config_editor.status_checking")}
        {:else if $editor.diagnostics.length > 0}
          <span class="text-destructive">
            {$t("settings.config_editor.status_errors", {
              values: { count: $editor.diagnostics.length },
            })}
          </span>
        {:else if dirty}
          {$t("settings.config_editor.status_modified")}
        {:else}
          {$t("settings.config_editor.status_saved")}
        {/if}
      </span>
      <span class="tabular-nums" data-testid="config-editor-cursor">
        {$t("settings.config_editor.cursor", { values: caret })}
      </span>
    </div>
  </div>

  {#if $editor.dialog?.kind === "defaults"}
    <ConfigDiffDialog
      open
      title={$t("settings.config_editor.defaults_title")}
      message={$t("settings.config_editor.defaults_message")}
      diff={$editor.dialog.diff}
      confirmLabel={$t("settings.config_editor.defaults")}
      unchangedLabel={$t("settings.config_editor.defaults_unchanged")}
      busy={$editor.saving}
      onconfirm={() => void editor.confirmDefaults()}
      onclose={editor.closeDialog}
      data-testid="config-editor-defaults-dialog"
    />
  {:else if $editor.dialog?.kind === "compare"}
    <ConfigDiffDialog
      open
      title={$t("settings.config_editor.compare_title")}
      message={$t("settings.config_editor.compare_message")}
      diff={$editor.dialog.diff}
      confirmLabel={$t("settings.config_editor.reload")}
      unchangedLabel={$t("settings.config_editor.compare_unchanged")}
      onconfirm={editor.reloadFromDisk}
      onclose={editor.closeDialog}
      data-testid="config-editor-compare-dialog"
    />
  {/if}

  <ConfirmDialog
    open={$editor.dialog?.kind === "restart"}
    title={$t("settings.config_editor.restart_title")}
    message={$t("settings.config_editor.restart_message", {
      values: { count: $tasksRunningCount },
    })}
    confirmLabel={$t("settings.config_editor.restart_confirm")}
    loading={$editor.restarting}
    onconfirm={() => void editor.confirmRestart()}
    onclose={editor.closeDialog}
    data-testid="config-editor-restart-dialog"
  />
{/if}
