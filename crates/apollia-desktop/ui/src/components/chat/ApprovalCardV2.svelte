<script lang="ts" module>
  /**
   * Canonical shape of a tool manifest slice surfaced inside an approval card.
   * Mirrors the Rust `ToolDescriptor` fields consumed by the UI — intentionally
   * narrow so that callers can pass any subset they have access to.
   */
  export interface ApprovalToolManifest {
    tool_name: string;
    /** Coarse-grained risk level ("low" | "medium" | "high" | "critical"). */
    risk_level?: ApprovalRiskLevel;
    /** Fine-grained score 0-10. Used to derive `risk_level` when missing. */
    risk_score?: number;
    /** Human-readable description of what accepting the tool will change. */
    impact_description?: string;
    /** When true, the user must supply a textual reason before rejecting. */
    reject_reason_required?: boolean;
  }

  export type ApprovalDecisionPayload =
    | { kind: "approve" }
    | { kind: "reject"; reason: string | null }
    | { kind: "always_accept"; scope: AlwaysAcceptScope };

  /**
   * Derive a risk level from a 0-10 score when the manifest did not declare one.
   * Mirrors `ApprovalRiskLevel::from_risk_score` in apollia-tools.
   */
  export function deriveRiskLevel(score: number | undefined): ApprovalRiskLevel {
    if (score === undefined) return "medium";
    if (score <= 3) return "low";
    if (score <= 6) return "medium";
    if (score <= 8) return "high";
    return "critical";
  }
</script>

<script lang="ts">
  import { onMount, tick } from "svelte";
  import { t } from "svelte-i18n";
  import { Ban } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Textarea } from "$lib/components/ui/textarea";
  import { addToast } from "$lib/components/ui/toast/store";
  import ApprovalRiskBadge, { type ApprovalRiskLevel } from "./ApprovalRiskBadge.svelte";
  import ApprovalTimer from "./ApprovalTimer.svelte";
  import ApprovalScopeSelect, { type AlwaysAcceptScope } from "./ApprovalScopeSelect.svelte";
  import { focusTrap } from "$lib/shortcuts/focusTrap";

  // Design-wise the card mirrors the frame provided by ReasoningCardShell
  // (left status bar, compact header, collapsible body). We re-implement the
  // frame here instead of composing the shell because the card needs the
  // `role="alertdialog"` semantics on its root element, which the shell does
  // not expose.

  interface Props {
    /** Manifest of the tool requesting approval. */
    manifest: ApprovalToolManifest;
    /** Arguments passed to the tool — rendered as JSON preview. */
    args?: Record<string, unknown> | null;
    /** Timestamp (epoch-ms) when the agent started waiting. */
    startedAtMs: number;
    /** Timeout budget in ms. `null` disables the progress bar. */
    totalTimeoutMs?: number | null;
    /** Current nesting depth (0..3). Added by parent for multi-level HITL. */
    nestedDepth?: number;
    /** Breadcrumb strings for nested approvals (root → leaf). */
    breadcrumb?: string[];
    /** Test id override. */
    testid?: string;
    /** Decision handler. Returns a promise that resolves on success. */
    onDecision: (payload: ApprovalDecisionPayload) => Promise<void>;
    /** Optional "Stop agent" handler (surfaced as ghost-destructive button). */
    onStop?: () => Promise<void> | void;
    /** Called when the timeout elapses. If `null`, the card stays open. */
    onExpired?: () => void;
  }

  let {
    manifest,
    args = null,
    startedAtMs,
    totalTimeoutMs = 300_000,
    nestedDepth = 0,
    breadcrumb = [],
    testid,
    onDecision,
    onStop,
    onExpired,
  }: Props = $props();

  // ── State ────────────────────────────────────────────────────────────────
  let processing = $state(false);
  let error = $state<string | null>(null);
  let resolved = $state(false);

  let showRejectForm = $state(false);
  let rejectReason = $state("");
  let showScopeDisclosure = $state(false);
  let selectedScope = $state<AlwaysAcceptScope>("this_session");

  let rootEl: HTMLDivElement | undefined = $state();

  // ── Derived ──────────────────────────────────────────────────────────────
  const riskLevel: ApprovalRiskLevel = $derived(
    manifest.risk_level ?? deriveRiskLevel(manifest.risk_score),
  );

  const MIN_REASON_LENGTH = 10;
  const REASON_REQUIRED = $derived(
    manifest.reject_reason_required === true || riskLevel === "critical",
  );
  const reasonValid = $derived(
    !REASON_REQUIRED || rejectReason.trim().length >= MIN_REASON_LENGTH,
  );

  const argsJson = $derived.by(() => {
    if (args === null || args === undefined) return null;
    try {
      return JSON.stringify(args, null, 2);
    } catch {
      return String(args);
    }
  });

  const MAX_DEPTH = 3;
  const boundedDepth = $derived(Math.min(nestedDepth, MAX_DEPTH));

  // ── Decision handlers ────────────────────────────────────────────────────
  async function dispatch(payload: ApprovalDecisionPayload): Promise<void> {
    if (processing || resolved) return;
    processing = true;
    error = null;
    try {
      await onDecision(payload);
      resolved = true;
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      processing = false;
    }
  }

  function handleApprove(): void {
    void dispatch({ kind: "approve" });
  }

  async function openRejectForm(): Promise<void> {
    showRejectForm = true;
    rejectReason = "";
    await tick();
    rootEl?.querySelector<HTMLTextAreaElement>("textarea")?.focus();
  }

  function handleReject(): void {
    if (REASON_REQUIRED && !reasonValid) return;
    const reason = rejectReason.trim() || null;
    void dispatch({ kind: "reject", reason });
  }

  function handleAlwaysAccept(): void {
    void dispatch({ kind: "always_accept", scope: selectedScope });
  }

  async function handleStop(): Promise<void> {
    if (!onStop) return;
    try {
      await onStop();
      resolved = true;
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  function handleExpired(): void {
    if (processing || resolved) return;
    addToast($t("approvals.toast.auto_denied"), "urgent");
    onExpired?.();
  }

  // ── A11y — focus management ──────────────────────────────────────────────
  // Tab cycling is handled by `use:focusTrap` on the root.
  // We still explicitly focus the primary action so screen readers
  // announce the approve button right after the card mounts.
  onMount(async () => {
    await tick();
    const firstBtn = rootEl?.querySelector<HTMLButtonElement>(
      "[data-testid='approval-v2-approve']",
    );
    firstBtn?.focus();
  });

  function handleKeydown(e: KeyboardEvent): void {
    if (processing || resolved) return;
    if (e.key === "Escape") {
      e.preventDefault();
      if (showRejectForm) {
        showRejectForm = false;
      } else if (REASON_REQUIRED) {
        void openRejectForm();
      } else {
        void dispatch({ kind: "reject", reason: null });
      }
    }
    if (e.key === "Enter" && (e.target as HTMLElement | null)?.tagName === "BUTTON") {
      // Default browser behaviour clicks the focused button.
    }
  }

  const borderClass = $derived.by(() => {
    switch (riskLevel) {
      case "critical":
        return "border-l-destructive";
      case "high":
        return "border-l-orange-500";
      case "medium":
        return "border-l-warning";
      default:
        return "border-l-success";
    }
  });

  /** Pixel indent for nested approvals — inline style to stay JIT-safe. */
  const indentPx = $derived(boundedDepth * 12);
</script>

{#if !resolved}
  <div
    bind:this={rootEl}
    role="alertdialog"
    aria-modal="true"
    use:focusTrap={{ initialFocus: "[data-testid='approval-v2-approve']" }}
    aria-labelledby="approval-title-{manifest.tool_name}"
    aria-describedby="approval-impact-{manifest.tool_name}"
    data-testid={testid ?? `approval-card-v2-${manifest.tool_name}`}
    data-risk-level={riskLevel}
    data-nested-depth={boundedDepth}
    onkeydown={handleKeydown}
    tabindex="-1"
    class="relative border-l-2 rounded-lg glass-card {borderClass}"
    style:margin-left="{indentPx}px"
  >
    <!-- Breadcrumb (nested approvals only) -->
    {#if breadcrumb.length > 0}
      <nav
        aria-label={$t("approvals.nested.breadcrumb_label")}
        class="px-3 pt-2 text-[10px] text-muted-foreground"
      >
        {#each breadcrumb as crumb, i}
          <span>{crumb}</span>
          {#if i < breadcrumb.length - 1}
            <span class="mx-1 opacity-50">›</span>
          {/if}
        {/each}
      </nav>
    {/if}

    <div class="px-3 py-2">
      <!-- Header: tool name + risk badge + timer -->
      <div class="flex items-center gap-2">
        <span
          id="approval-title-{manifest.tool_name}"
          class="flex-1 truncate text-[13px] font-medium font-mono"
        >
          {manifest.tool_name}
        </span>
        <ApprovalRiskBadge level={riskLevel} />
      </div>

      <div class="mt-1.5">
        <ApprovalTimer
          startedAt={startedAtMs}
          totalMs={totalTimeoutMs}
          onExpired={handleExpired}
        />
      </div>

      <!-- Impact description -->
      {#if manifest.impact_description}
        <p
          id="approval-impact-{manifest.tool_name}"
          class="mt-2 text-[12px] text-foreground"
          data-testid="approval-impact-desc"
        >
          {manifest.impact_description}
        </p>
      {/if}

      <!-- Args preview (collapsible, max 320 px) -->
      {#if argsJson}
        <details class="mt-2" data-testid="approval-args-details">
          <summary
            class="cursor-pointer text-[11px] font-medium text-muted-foreground hover:text-foreground"
          >
            {$t("approvals.args_preview")}
          </summary>
          <pre
            class="mt-1 max-h-[320px] overflow-auto rounded glass-inset px-2 py-1.5 font-mono text-[11px] text-foreground"
            data-testid="approval-args-json"
          >{argsJson}</pre>
        </details>
      {/if}

      <!-- Reject reason form -->
      {#if showRejectForm}
        <div class="mt-2 rounded-md border border-destructive/30 bg-destructive/5 p-2">
          <label
            for="approval-reject-reason-{manifest.tool_name}"
            class="mb-1 block text-[11px] font-medium text-destructive"
          >
            {#if REASON_REQUIRED}
              {$t("approvals.reject.reason_required", {
                values: { min: MIN_REASON_LENGTH },
              })}
            {:else}
              {$t("approvals.reject.reason_optional")}
            {/if}
          </label>
          <Textarea
            id="approval-reject-reason-{manifest.tool_name}"
            bind:value={rejectReason}
            rows={2}
            placeholder={$t("approvals.reject.placeholder")}
            data-testid="approval-reject-reason"
          />
          <p class="mt-1 text-[10px] text-muted-foreground">
            {rejectReason.trim().length} / {MIN_REASON_LENGTH}
          </p>
        </div>
      {/if}

      <!-- Scope disclosure (hidden behind the tertiary "Always accept") -->
      {#if showScopeDisclosure}
        <div class="mt-2" data-testid="approval-scope-disclosure">
          <ApprovalScopeSelect
            bind:value={selectedScope}
            disabled={processing}
          />
        </div>
      {/if}

      <!-- Error -->
      {#if error}
        <p
          class="mt-2 text-[11px] text-destructive"
          role="alert"
          data-testid="approval-error"
        >
          {error}
        </p>
      {/if}

      <!-- Actions -->
      <div class="mt-2.5 flex flex-wrap items-center gap-2">
        {#if !showRejectForm}
          <Button
            size="sm"
            variant="default"
            class="h-7 px-4 text-[11px]"
            disabled={processing}
            onclick={handleApprove}
            data-testid="approval-v2-approve"
          >
            {$t("approvals.action.approve")}
          </Button>
          <Button
            size="sm"
            variant="ghost"
            class="h-7 px-3 text-[11px] text-destructive hover:bg-destructive/10"
            disabled={processing}
            onclick={() => {
              if (REASON_REQUIRED) {
                void openRejectForm();
              } else {
                void dispatch({ kind: "reject", reason: null });
              }
            }}
            data-testid="approval-v2-reject"
          >
            {$t("approvals.action.reject")}
          </Button>
          <Button
            size="sm"
            variant="ghost"
            class="h-7 px-3 text-[11px] text-primary"
            disabled={processing}
            onclick={() => (showScopeDisclosure = !showScopeDisclosure)}
            data-testid="approval-v2-always-toggle"
            aria-expanded={showScopeDisclosure}
          >
            {$t("approvals.action.always_accept")}
          </Button>
          {#if showScopeDisclosure}
            <Button
              size="sm"
              variant="secondary"
              class="h-7 px-3 text-[11px]"
              disabled={processing}
              onclick={handleAlwaysAccept}
              data-testid="approval-v2-always-confirm"
            >
              {$t("approvals.action.confirm_always")}
            </Button>
          {/if}
          {#if onStop}
            <Button
              size="sm"
              variant="ghost"
              class="ml-auto h-7 px-3 text-[11px] text-destructive"
              disabled={processing}
              onclick={handleStop}
              data-testid="approval-v2-stop"
              aria-label={$t("approvals.action.stop_agent")}
            >
              <Ban class="h-3 w-3" aria-hidden="true" />
              <span class="ml-1">{$t("approvals.action.stop")}</span>
            </Button>
          {/if}
        {:else}
          <Button
            size="sm"
            variant="destructive"
            class="h-7 px-4 text-[11px]"
            disabled={processing || !reasonValid}
            onclick={handleReject}
            data-testid="approval-v2-reject-confirm"
          >
            {$t("approvals.action.confirm_reject")}
          </Button>
          <Button
            size="sm"
            variant="ghost"
            class="h-7 px-3 text-[11px]"
            disabled={processing}
            onclick={() => {
              showRejectForm = false;
              rejectReason = "";
            }}
            data-testid="approval-v2-reject-cancel"
          >
            {$t("common.cancel")}
          </Button>
        {/if}
      </div>
    </div>
  </div>
{/if}
