/**
 * The client-side send throttle of the composer.
 *
 * `ChatRateLimiter` decides; this factory carries the decision as live state so
 * the Send button can be pre-disabled and a countdown shown until the cooldown
 * elapses, instead of the operator discovering the refusal by clicking.
 */
import { get } from "svelte/store";
import { t } from "svelte-i18n";
import { ChatRateLimiter } from "$lib/chat/rateLimit";

export interface ComposerRateLimit {
  /** `null` while sending is allowed. */
  readonly blockedReason: "too_fast" | "too_many" | null;
  /** The countdown sentence, or `null` while sending is allowed. */
  readonly status: string | null;
  readonly tone: "neutral" | "warn";
  /**
   * Record a send and arm the cooldown, or refuse it. `false` means the caller
   * must not send: the countdown is already running.
   */
  admitSend(): boolean;
}

export function createComposerRateLimit(): ComposerRateLimit {
  const limiter = new ChatRateLimiter();
  let blockedMs = $state(0);
  let blockedReason = $state<"too_fast" | "too_many" | null>(null);
  let status = $state<string | null>(null);
  let tone = $state<"neutral" | "warn">("neutral");
  let blockTimer: ReturnType<typeof setInterval> | undefined;

  function refresh(): void {
    const check = limiter.check();
    if (check.allowed) {
      blockedMs = 0;
      blockedReason = null;
      status = null;
      tone = "neutral";
    } else {
      blockedMs = check.retryAfterMs ?? 0;
      blockedReason = check.reason ?? null;
      status = check.reason === "too_fast"
        ? get(t)("chat.rate_limit.too_fast_countdown", {
            values: { s: Math.ceil(blockedMs / 1000) },
          })
        : get(t)("chat.rate_limit.too_many_countdown", {
            values: { s: Math.ceil(blockedMs / 1000) },
          });
      tone = "warn";
    }
  }

  function ensureTimer(): void {
    if (blockTimer !== undefined) return;
    blockTimer = setInterval(() => {
      refresh();
      if (blockedReason === null && blockTimer !== undefined) {
        clearInterval(blockTimer);
        blockTimer = undefined;
      }
    }, 200);
  }

  return {
    get blockedReason() {
      return blockedReason;
    },
    get status() {
      return status;
    },
    get tone() {
      return tone;
    },
    admitSend(): boolean {
      const check = limiter.check();
      if (!check.allowed) {
        refresh();
        ensureTimer();
        return false;
      }
      limiter.record();
      // After a successful send, the min-interval cooldown kicks in - surface it.
      refresh();
      ensureTimer();
      return true;
    },
  };
}
