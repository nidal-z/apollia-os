<script lang="ts">
  /**
   * Filesystem HITL modal — refonte.
   *
   * Behaviour:
   *   - progress bar timeout (via ApprovalTimer)
   *   - toast on auto-deny so the operator sees why the action failed
   *   - convention "close = deny" conservée, documentée dans DS
   *   - i18n du mot CONFIRM via `hitl.fs.critical_confirm_word`
   *   - autofocus sur Deny (destructif par défaut) + trap via Dialog
   *   - scope "always allow" exposé via ApprovalScopeSelect
   *
   * The heavy lifting of the decision is still `respond_hitl_filesystem`
   * on the Rust side ; only the UI shell changes.
   */

  import { onMount, onDestroy, tick } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { t } from "svelte-i18n";
  import { AlertTriangle, FileText, ShieldAlert, Trash2 } from "lucide-svelte";
  import Dialog from "$lib/components/ui/dialog/Dialog.svelte";
  import { Button } from "$lib/components/ui/button";
  import { Textarea } from "$lib/components/ui/textarea";
  import { addToast } from "$lib/components/ui/toast/store";
  import ApprovalRiskBadge, { type ApprovalRiskLevel } from "./ApprovalRiskBadge.svelte";
  import ApprovalTimer from "./ApprovalTimer.svelte";
  import ApprovalScopeSelect, { type AlwaysAcceptScope } from "./ApprovalScopeSelect.svelte";

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

  const TIMEOUT_MS = 300_000;

  // ── Component state ──────────────────────────────────────────────────────
  let open = $state(false);
  let pending = $state<HitlFsPayload | null>(null);
  let processing = $state(false);
  let error = $state<string | null>(null);
  let startedAt = $state<number>(Date.now());
  let criticalInput = $state("");
  let showRejectReason = $state(false);
  let rejectReason = $state("");
  let showScope = $state(false);
  let scope = $state<AlwaysAcceptScope>("this_session");

  let unlistenFn: UnlistenFn | undefined;

  // ── Derived ──────────────────────────────────────────────────────────────
  const isCritical = $derived(pending?.level === "critical");
  const riskLevel: ApprovalRiskLevel = $derived(
    (pending?.level as ApprovalRiskLevel | undefined) ?? "medium",
  );

  /** i18n-aware CONFIRM comparison. Falls back to literal "CONFIRM". */
  const confirmWord = $derived(
    ($t("hitl.fs.critical_confirm_word") as string | undefined) ?? "CONFIRM",
  );
  const canApprove = $derived(
    !processing &&
      (!isCritical || criticalInput.trim().toUpperCase() === confirmWord.toUpperCase()),
  );

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

  // ── Lifecycle ────────────────────────────────────────────────────────────
  onMount(async () => {
    unlistenFn = await listen<HitlFsPayload>("hitl-fs-required", (event) => {
      if (event.payload.session_id !== sessionId) return;
      openModal(event.payload);
    });
  });

  onDestroy(() => {
    unlistenFn?.();
  });

  async function openModal(payload: HitlFsPayload): Promise<void> {
    pending = payload;
    open = true;
    processing = false;
    error = null;
    criticalInput = "";
    rejectReason = "";
    showRejectReason = false;
    showScope = false;
    startedAt = Date.now();
    // autofocus Deny (destructive default) once the dialog renders.
    await tick();
    document
      .querySelector<HTMLButtonElement>("[data-testid='hitl-fs-deny']")
      ?.focus();
  }

  function closeModal(): void {
    open = false;
    pending = null;
    processing = false;
    error = null;
    criticalInput = "";
    rejectReason = "";
    showRejectReason = false;
    showScope = false;
  }

  function handleAutoDeny(): void {
    if (!pending || processing) return;
    // surface the auto-deny so the operator notices the timeout
    addToast($t("hitl.fs.auto_denied"), "urgent");
    void sendDecision("deny", null);
  }

  // ── Decision handling ────────────────────────────────────────────────────
  async function sendDecision(
    kind: "approve" | "deny" | "always_allow",
    reason: string | null = null,
  ): Promise<void> {
    if (!pending || processing) return;
    processing = true;
    error = null;

    const requestId = pending.request_id;
    const op = pending.op;
    const level = pending.level;

    let decision: unknown;
    if (kind === "approve") {
      decision = { decision: "approve" };
    } else if (kind === "deny") {
      decision = { decision: "deny", reason };
    } else {
      decision = { decision: "always_allow", op, level, scope };
    }

    try {
      await invoke("respond_hitl_filesystem", { requestId, decision });
      closeModal();
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
      processing = false;
    }
  }

  function handleDenyClick(): void {
    if (showRejectReason) {
      void sendDecision("deny", rejectReason.trim() || null);
    } else if (isCritical) {
      showRejectReason = true;
    } else {
      void sendDecision("deny", null);
    }
  }
</script>

<Dialog
  {open}
  onclose={() => sendDecision("deny", null)}
  size="lg"
  title={$t("hitl.fs.title")}
  data-testid="hitl-fs-modal"
>
  {#if pending}
    <!-- Subtitle -->
    <p class="mb-3 text-sm text-muted-foreground">{$t("hitl.fs.subtitle")}</p>

    <!-- Op + level row (risk badge + op) -->
    <div class="mb-3 flex items-center gap-2">
      <ApprovalRiskBadge level={riskLevel} />
      <span
        class="inline-flex items-center gap-1 rounded-md bg-muted/50 px-2 py-0.5 text-[11px] font-medium text-foreground"
        data-testid="hitl-fs-op"
      >
        <OpIcon class="h-3 w-3" aria-hidden="true" />
        {opLabel}
      </span>
      <!-- Legacy alert icon kept for visual familiarity (non-critical) -->
      {#if !isCritical}
        <AlertTriangle class="h-3 w-3 text-warning" aria-hidden="true" />
      {/if}
    </div>

    <!-- Timer + progress bar -->
    <div class="mb-3">
      <ApprovalTimer
        startedAt={startedAt}
        totalMs={TIMEOUT_MS}
        onExpired={handleAutoDeny}
      />
    </div>

    <!-- Path -->
    <div class="mb-3">
      <p class="mb-1 text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
        {$t("hitl.fs.path_label")}
      </p>
      <code
        class="block break-all rounded-md bg-muted/50 px-3 py-1.5 font-mono text-[11px] text-foreground"
        data-testid="hitl-fs-path"
      >{pending.path}</code>
    </div>

    <!-- Preview — unchanged scenarios (diff / content / mode) -->
    {#if pending.preview.kind === "diff"}
      {@const diff = pending.preview}
      <div class="mb-3">
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
      <div class="mb-3">
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
      <div class="mb-3">
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

    <!-- Critical: CONFIRM input (i18n-aware via `hitl.fs.critical_confirm_word`) -->
    {#if isCritical}
      <div class="mb-3 rounded-md border border-destructive/30 bg-destructive/5 px-3 py-3">
        <label class="mb-1.5 block text-[11px] font-medium text-destructive" for="hitl-fs-critical-input">
          {$t("hitl.fs.critical_confirm_label", { values: { word: confirmWord } })}
        </label>
        <input
          id="hitl-fs-critical-input"
          type="text"
          bind:value={criticalInput}
          placeholder={confirmWord}
          class="w-full rounded-md border border-destructive/30 bg-background px-3 py-1.5 font-mono text-sm outline-none focus:border-destructive/60"
          data-testid="hitl-fs-critical-input"
          autocomplete="off"
          spellcheck={false}
        />
      </div>
    {/if}

    <!-- Reject reason (critical only or operator-invoked) -->
    {#if showRejectReason}
      <div class="mb-3 rounded-md border border-destructive/30 bg-destructive/5 p-2">
        <label
          for="hitl-fs-reject-reason"
          class="mb-1 block text-[11px] font-medium text-destructive"
        >
          {$t("hitl.fs.reject_reason_label")}
        </label>
        <Textarea
          id="hitl-fs-reject-reason"
          bind:value={rejectReason}
          rows={2}
          placeholder={$t("hitl.fs.reject_reason_placeholder")}
          data-testid="hitl-fs-reject-reason"
        />
      </div>
    {/if}

    <!-- Always allow scope disclosure -->
    {#if showScope}
      <div class="mb-3" data-testid="hitl-fs-scope">
        <ApprovalScopeSelect bind:value={scope} disabled={processing} />
      </div>
    {/if}

    <!-- Error -->
    {#if error}
      <p class="mb-2 text-[11px] text-destructive" role="alert" data-testid="hitl-fs-error">
        {error}
      </p>
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
        onclick={handleDenyClick}
        data-testid="hitl-fs-deny"
      >
        {showRejectReason ? $t("hitl.fs.deny_confirm") : $t("hitl.fs.deny")}
      </Button>
      <Button
        variant="ghost"
        size="sm"
        class="ml-auto h-8 px-3 text-[11px] text-primary"
        disabled={!canApprove}
        onclick={() => {
          if (showScope) {
            void sendDecision("always_allow");
          } else {
            showScope = true;
          }
        }}
        data-testid="hitl-fs-always-allow"
        aria-expanded={showScope}
        title={$t("hitl.fs.always_allow_desc", {
          values: { op: opLabel, level: $t(`approvals.risk.${riskLevel}`) },
        })}
      >
        {showScope ? $t("hitl.fs.always_allow_confirm") : $t("hitl.fs.always_allow_session")}
      </Button>
    </div>
  {/if}
</Dialog>
