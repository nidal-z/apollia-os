<script lang="ts">
  /**
   * Per-session plan-mode toggle chip for the chat header meta row.
   *
   * Reflects the active session's plan-mode state and toggles it through the
   * `set_plan_mode` command (via `$lib/ipc/planMode`). Optimistic: it flips
   * immediately and reverts to the real state if the command fails.
   *
   * Operator persona shows the icon + colored state only; Builder persona adds
   * a textual label next to the icon.
   */
  import { t } from "svelte-i18n";
  import { ListChecks } from "lucide-svelte";
  import { setPlanMode } from "$lib/ipc/planMode";
  import { addToast } from "$lib/components/ui/toast/store";

  interface Props {
    sessionId: string;
    /** Current plan-mode state of the session. */
    planMode: boolean;
    /** Disabled when the session is closed. */
    disabled?: boolean;
    /** Notifies the parent so it can refresh the session detail. */
    onchange?: (enabled: boolean) => void;
  }

  let { sessionId, planMode, disabled = false, onchange }: Props = $props();

  let pending = $state(false);

  async function toggle(): Promise<void> {
    if (pending || disabled) return;
    const next = !planMode;
    pending = true;
    try {
      await setPlanMode(sessionId, next);
      onchange?.(next);
    } catch {
      // Real state is unchanged on failure: nothing to revert, just surface it.
      addToast($t("chat.planMode.toggleError"), "error");
    } finally {
      pending = false;
    }
  }
</script>

<button
  type="button"
  onclick={toggle}
  disabled={disabled || pending}
  aria-pressed={planMode}
  aria-label={planMode
    ? $t("chat.planMode.toggle.on")
    : $t("chat.planMode.toggle.off")}
  data-active={planMode}
  data-testid="chat-plan-mode-chip"
  class="inline-flex items-center gap-1.5 rounded-md border px-2 py-1 text-caption font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 disabled:cursor-not-allowed disabled:opacity-50 {planMode
    ? 'border-primary/40 bg-primary/12 text-primary'
    : 'border-border bg-transparent text-muted-foreground hover:text-foreground hover:bg-muted/50'}"
>
  <ListChecks size={12} class="shrink-0" aria-hidden="true" />
  <span class="truncate">{$t("chat.planMode.chipLabel")}</span>
</button>
