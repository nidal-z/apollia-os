<script lang="ts">
  /**
   * 2-step confirmation dialog for destructive Settings actions
   * Step 1 is a classical alertdialog; Step 2 requires the
   * user to type the exact `confirmWord` (RESET / DELETE / FACTORY) before
   * the Confirm button unlocks. Factory-reset-style callers can also set
   * `lockMs` to disable Confirm for N ms after the dialog mounts, a
   * second safeguard against accidental double-click.
   */
  import { onDestroy } from "svelte";
  import { t } from "svelte-i18n";
  import Dialog from "$lib/components/ui/dialog/Dialog.svelte";
  import DialogFooter from "$lib/components/ui/dialog/DialogFooter.svelte";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { AlertTriangle, CheckCircle2, XCircle, Lock } from "lucide-svelte";
  import { Spinner } from "$lib/components/ui/progress";

  interface Props {
    open: boolean;
    title: string;
    description: string;
    /** Word that must be typed on step 2 (case-sensitive). */
    confirmWord: string;
    /** Label for the final destructive button. */
    confirmLabel: string;
    /** Short danger banner above the input on step 2. */
    warning?: string;
    /** Loading state while the async action runs. */
    loading?: boolean;
    /** If > 0, Confirm is disabled for the first `lockMs` ms (factory reset). */
    lockMs?: number;
    onclose: () => void;
    onconfirm: () => void | Promise<void>;
    "data-testid"?: string;
  }

  let {
    open,
    title,
    description,
    confirmWord,
    confirmLabel,
    warning,
    loading = false,
    lockMs = 0,
    onclose,
    onconfirm,
    "data-testid": dataTestId,
  }: Props = $props();

  let step = $state<1 | 2>(1);
  let typed = $state("");
  let locked = $state(false);
  let lockRemaining = $state(0);
  let lockTimer: ReturnType<typeof setTimeout> | null = null;
  let lockInterval: ReturnType<typeof setInterval> | null = null;

  function clearLockTimers() {
    if (lockTimer) {
      clearTimeout(lockTimer);
      lockTimer = null;
    }
    if (lockInterval) {
      clearInterval(lockInterval);
      lockInterval = null;
    }
  }

  function resetState() {
    step = 1;
    typed = "";
    locked = false;
    lockRemaining = 0;
    clearLockTimers();
  }

  function handleClose() {
    resetState();
    onclose();
  }

  function goToStep2() {
    step = 2;
    typed = "";
    if (lockMs > 0) {
      locked = true;
      lockRemaining = Math.ceil(lockMs / 1000);
      lockTimer = setTimeout(() => {
        locked = false;
        lockRemaining = 0;
      }, lockMs);
      lockInterval = setInterval(() => {
        lockRemaining = Math.max(0, lockRemaining - 1);
        if (lockRemaining <= 0 && lockInterval) {
          clearInterval(lockInterval);
          lockInterval = null;
        }
      }, 1000);
    }
  }

  async function handleConfirm() {
    if (typed !== confirmWord || locked) return;
    await onconfirm();
    resetState();
  }

  $effect(() => {
    if (!open) resetState();
  });

  onDestroy(clearLockTimers);

  let matches = $derived(typed === confirmWord);
  let canConfirm = $derived(matches && !locked && !loading);
</script>

<Dialog
  {open}
  onclose={handleClose}
  size="sm"
  {title}
  data-testid={dataTestId}
  role="alertdialog"
>
  {#if step === 1}
    <div class="flex items-start gap-3" data-testid={dataTestId ? `${dataTestId}-step1` : undefined}>
      <AlertTriangle
        size={20}
        strokeWidth={1.75}
        class="mt-0.5 flex-shrink-0 text-destructive"
        aria-hidden="true"
      />
      <p class="text-sm text-muted-foreground">{description}</p>
    </div>
    <DialogFooter>
      <Button
        variant="outline"
        onclick={handleClose}
        data-testid={dataTestId ? `${dataTestId}-cancel` : undefined}
      >
        {$t("common.cancel")}
      </Button>
      <Button
        variant="destructive"
        onclick={goToStep2}
        data-testid={dataTestId ? `${dataTestId}-continue` : undefined}
      >
        {$t("settings.danger.continue")}
      </Button>
    </DialogFooter>
  {:else}
    <div class="space-y-3" data-testid={dataTestId ? `${dataTestId}-step2` : undefined}>
      {#if warning}
        <div
          class="rounded-md border border-destructive/40 bg-destructive/5 px-3 py-2 text-xs text-destructive"
        >
          {warning}
        </div>
      {/if}
      <p class="text-sm text-muted-foreground">
        {@html $t("settings.danger.type_to_confirm", { values: { word: `<code class="rounded bg-muted px-1 py-0.5 font-mono text-foreground">${confirmWord}</code>` } })}
      </p>
      <Input
        type="text"
        bind:value={typed}
        placeholder={confirmWord}
        autocomplete="off"
        autocorrect="off"
        spellcheck={false}
        onpaste={(e: ClipboardEvent) => e.preventDefault()}
        aria-label={$t("settings.danger.confirm_input_aria", { values: { word: confirmWord } })}
        data-testid={dataTestId ? `${dataTestId}-input` : undefined}
      />
      {#if matches}
        <p
          class="flex items-center gap-1.5 text-caption text-success"
          data-testid={dataTestId ? `${dataTestId}-match` : undefined}
        >
          <CheckCircle2 size={13} strokeWidth={2} aria-hidden="true" />
          {$t("settings.danger.confirm_match")}
        </p>
      {:else if typed.length > 0}
        <p
          class="flex items-center gap-1.5 text-caption text-destructive"
          data-testid={dataTestId ? `${dataTestId}-mismatch` : undefined}
        >
          <XCircle size={13} strokeWidth={2} aria-hidden="true" />
          {$t("settings.danger.confirm_mismatch")}
        </p>
      {/if}
    </div>
    <DialogFooter>
      {#if locked && lockRemaining > 0}
        <span
          class="mr-auto inline-flex items-center gap-1.5 self-center text-caption text-muted-foreground"
          aria-live="polite"
          data-testid={dataTestId ? `${dataTestId}-lock-hint` : undefined}
        >
          <Lock size={13} strokeWidth={1.75} aria-hidden="true" />
          {$t("settings.danger.locked_hint", { values: { seconds: lockRemaining } })}
        </span>
      {/if}
      <Button
        variant="outline"
        onclick={handleClose}
        data-testid={dataTestId ? `${dataTestId}-cancel` : undefined}
      >
        {$t("common.cancel")}
      </Button>
      <Button
        variant="destructive"
        onclick={handleConfirm}
        disabled={!canConfirm}
        data-testid={dataTestId ? `${dataTestId}-confirm` : undefined}
      >
        {#if loading}
          <Spinner class="mr-2 h-4 w-4" />
        {/if}
        {confirmLabel}
      </Button>
    </DialogFooter>
  {/if}
</Dialog>
