<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { t } from "svelte-i18n";
  import { AlertTriangle, FileText, ShieldAlert, Trash2 } from "lucide-svelte";
  import Dialog from "$lib/components/ui/dialog/Dialog.svelte";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    /** Chat session — only requests for this session will open the modal. */
    sessionId: string;
  }

  let { sessionId }: Props = $props();

  // ── Event payload shape (mirrors HitlFsRequiredPayload from Rust) ─────────
  interface DiffPreview {
    kind: "diff";
    before: string;
    after: string;
    truncated: boolean;
  }
  interface ContentPreview {
    kind: "content";
    content: string;
    size_bytes: number;
    truncated: boolean;
  }
  interface ModePreview {
    kind: "mode";
    before: number;
    after: number;
  }
  type FilesystemPreview = DiffPreview | ContentPreview | ModePreview;

  interface HitlFsPayload {
    request_id: string;
    session_id: string;
    level: "medium" | "high" | "critical";
    op: string;
    path: string;
    preview: FilesystemPreview;
  }

  // ── Component state ────────────────────────────────────────────────────────
  let open = $state(false);
  let pending = $state<HitlFsPayload | null>(null);
  let processing = $state(false);
  let error = $state<string | null>(null);
  /** Input for critical confirmation — user must type "CONFIRM". */
  let criticalInput = $state("");
  /** Countdown in seconds before auto-deny. */
  let timeoutSecs = $state(300);

  let unlistenFn: UnlistenFn | undefined;
  let countdownInterval: ReturnType<typeof setInterval> | undefined;

  // ── Derived ───────────────────────────────────────────────────────────────
  const isCritical = $derived(pending?.level === "critical");
  const canApprove = $derived(
    !processing && (!isCritical || criticalInput.trim().toUpperCase() === "CONFIRM"),
  );

  const levelClass = $derived.by(() => {
    switch (pending?.level) {
      case "critical":
        return "bg-destructive/10 text-destructive border-destructive/30";
      case "high":
        return "bg-orange-500/10 text-orange-500 border-orange-500/30";
      default:
        return "bg-warning/10 text-warning border-warning/30";
    }
  });

  const levelLabel = $derived.by(() => {
    switch (pending?.level) {
      case "critical":
        return $t("hitl.fs.level_critical");
      case "high":
        return $t("hitl.fs.level_high");
      default:
        return $t("hitl.fs.level_medium");
    }
  });

  const opLabel = $derived.by(() => {
    switch (pending?.op) {
      case "write":
        return $t("hitl.fs.op_write");
      case "delete":
        return $t("hitl.fs.op_delete");
      case "chmod":
        return $t("hitl.fs.op_chmod");
      case "read":
        return $t("hitl.fs.op_read");
      default:
        return pending?.op ?? "";
    }
  });

  const OpIcon = $derived.by(() => {
    switch (pending?.op) {
      case "delete":
        return Trash2;
      case "chmod":
        return ShieldAlert;
      default:
        return FileText;
    }
  });

  // ── Lifecycle ──────────────────────────────────────────────────────────────
  onMount(async () => {
    unlistenFn = await listen<HitlFsPayload>("hitl-fs-required", (event) => {
      if (event.payload.session_id !== sessionId) return;
      openModal(event.payload);
    });
  });

  onDestroy(() => {
    unlistenFn?.();
    stopCountdown();
  });

  function openModal(payload: HitlFsPayload): void {
    pending = payload;
    open = true;
    processing = false;
    error = null;
    criticalInput = "";
    startCountdown();
  }

  function closeModal(): void {
    open = false;
    pending = null;
    processing = false;
    error = null;
    criticalInput = "";
    stopCountdown();
  }

  function startCountdown(): void {
    timeoutSecs = 300;
    stopCountdown();
    countdownInterval = setInterval(() => {
      timeoutSecs -= 1;
      if (timeoutSecs <= 0) {
        stopCountdown();
        // Auto-deny on timeout
        if (pending) {
          void sendDecision("deny");
        }
      }
    }, 1000);
  }

  function stopCountdown(): void {
    if (countdownInterval !== undefined) {
      clearInterval(countdownInterval);
      countdownInterval = undefined;
    }
  }

  // ── Decision handling ──────────────────────────────────────────────────────
  async function sendDecision(
    kind: "approve" | "deny" | "always_allow_session",
  ): Promise<void> {
    if (!pending || processing) return;
    processing = true;
    error = null;
    stopCountdown();

    const requestId = pending.request_id;
    const op = pending.op;
    const level = pending.level;

    let decision: unknown;
    if (kind === "approve") {
      decision = { decision: "approve" };
    } else if (kind === "deny") {
      decision = { decision: "deny" };
    } else {
      decision = { decision: "always_allow_session", op, level };
    }

    try {
      await invoke("respond_hitl_filesystem", { requestId, decision });
      closeModal();
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
      processing = false;
    }
  }
</script>

<Dialog
  {open}
  onclose={() => sendDecision("deny")}
  size="lg"
  title={$t("hitl.fs.title")}
  data-testid="hitl-fs-modal"
>
  {#if pending}
    <!-- Subtitle -->
    <p class="mb-4 text-sm text-muted-foreground">{$t("hitl.fs.subtitle")}</p>

    <!-- Op + level row -->
    <div class="mb-3 flex items-center gap-2">
      <!-- Level badge -->
      <span
        class="inline-flex items-center gap-1 rounded-md border px-2 py-0.5 text-[11px] font-semibold {levelClass}"
        data-testid="hitl-fs-level"
      >
        <AlertTriangle class="h-3 w-3" />
        {levelLabel}
      </span>
      <!-- Op badge -->
      <span
        class="inline-flex items-center gap-1 rounded-md bg-muted/50 px-2 py-0.5 text-[11px] font-medium text-foreground"
        data-testid="hitl-fs-op"
      >
        <OpIcon class="h-3 w-3" />
        {opLabel}
      </span>
    </div>

    <!-- Path -->
    <div class="mb-4">
      <p class="mb-1 text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
        {$t("hitl.fs.path_label")}
      </p>
      <code
        class="block break-all rounded-md bg-muted/50 px-3 py-1.5 font-mono text-[11px] text-foreground"
        data-testid="hitl-fs-path"
      >{pending.path}</code>
    </div>

    <!-- Preview -->
    {#if pending.preview.kind === "diff"}
      {@const diff = pending.preview}
      <div class="mb-4">
        <p class="mb-1 text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
          {$t("hitl.fs.preview_diff")}
        </p>
        <div class="overflow-hidden rounded-md border border-border/40 text-[11px] font-mono">
          {#if diff.before}
            <div class="border-b border-border/30 bg-destructive/5 px-3 py-1.5">
              <span class="mb-1 block text-[10px] font-semibold text-muted-foreground/60">
                {$t("hitl.fs.preview_before")}
              </span>
              <pre class="max-h-32 overflow-auto whitespace-pre-wrap break-all text-destructive/80">{diff.before}</pre>
            </div>
          {/if}
          <div class="bg-success/5 px-3 py-1.5">
            <span class="mb-1 block text-[10px] font-semibold text-muted-foreground/60">
              {$t("hitl.fs.preview_after")}
            </span>
            <pre class="max-h-32 overflow-auto whitespace-pre-wrap break-all text-success/80">{diff.after}</pre>
          </div>
        </div>
        {#if diff.truncated}
          <p class="mt-1 text-[10px] text-muted-foreground/60">{$t("hitl.fs.preview_truncated")}</p>
        {/if}
      </div>
    {:else if pending.preview.kind === "content"}
      {@const content = pending.preview}
      <div class="mb-4">
        <p class="mb-1 text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
          {$t("hitl.fs.preview_content")}
          <span class="ml-1 font-normal normal-case text-muted-foreground/60">
            ({$t("hitl.fs.preview_size", { values: { size: content.size_bytes } })})
          </span>
        </p>
        <pre
          class="max-h-48 overflow-auto rounded-md bg-muted/40 px-3 py-2 font-mono text-[11px] whitespace-pre-wrap break-all text-foreground"
          data-testid="hitl-fs-content-preview"
        >{content.content}</pre>
        {#if content.truncated}
          <p class="mt-1 text-[10px] text-muted-foreground/60">{$t("hitl.fs.preview_truncated")}</p>
        {/if}
      </div>
    {:else if pending.preview.kind === "mode"}
      {@const mode = pending.preview}
      <div class="mb-4">
        <p class="mb-1 text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
          {$t("hitl.fs.preview_mode")}
        </p>
        <p
          class="rounded-md bg-muted/40 px-3 py-2 font-mono text-[12px] text-foreground"
          data-testid="hitl-fs-mode-preview"
        >
          {$t("hitl.fs.preview_mode_label", {
            values: {
              before: "0o" + mode.before.toString(8),
              after: "0o" + mode.after.toString(8),
            },
          })}
        </p>
      </div>
    {/if}

    <!-- Critical: confirmation input -->
    {#if isCritical}
      <div class="mb-4 rounded-md border border-destructive/30 bg-destructive/5 px-3 py-3">
        <label class="mb-1.5 block text-[11px] font-medium text-destructive" for="hitl-fs-critical-input">
          {$t("hitl.fs.critical_confirm_label")}
        </label>
        <input
          id="hitl-fs-critical-input"
          type="text"
          bind:value={criticalInput}
          placeholder={$t("hitl.fs.critical_confirm_placeholder")}
          class="w-full rounded-md border border-destructive/30 bg-background px-3 py-1.5 font-mono text-sm outline-none focus:border-destructive/60"
          data-testid="hitl-fs-critical-input"
          autocomplete="off"
          spellcheck={false}
        />
      </div>
    {/if}

    <!-- Timeout warning -->
    {#if timeoutSecs <= 30}
      <p class="mb-3 text-[11px] text-warning" data-testid="hitl-fs-timeout">
        {$t("hitl.fs.timeout_warning", { values: { secs: timeoutSecs } })}
      </p>
    {/if}

    <!-- Error -->
    {#if error}
      <p class="mb-3 text-[11px] text-destructive" data-testid="hitl-fs-error">{error}</p>
    {/if}

    <!-- Action buttons -->
    <div class="flex items-center gap-2">
      <Button
        variant="outline"
        size="sm"
        class="h-8 px-4 text-xs"
        disabled={!canApprove}
        onclick={() => sendDecision("approve")}
        data-testid="hitl-fs-approve"
      >
        {$t("hitl.fs.approve")}
      </Button>
      <Button
        variant="ghost"
        size="sm"
        class="h-8 px-4 text-xs text-destructive hover:bg-destructive/10 hover:text-destructive"
        disabled={processing}
        onclick={() => sendDecision("deny")}
        data-testid="hitl-fs-deny"
      >
        {$t("hitl.fs.deny")}
      </Button>
      <Button
        variant="ghost"
        size="sm"
        class="ml-auto h-8 px-3 text-[11px] text-primary"
        disabled={!canApprove}
        onclick={() => sendDecision("always_allow_session")}
        data-testid="hitl-fs-always-allow"
        title={$t("hitl.fs.always_allow_desc", {
          values: { op: opLabel, level: levelLabel },
        })}
      >
        {$t("hitl.fs.always_allow_session")}
      </Button>
    </div>
  {/if}
</Dialog>
