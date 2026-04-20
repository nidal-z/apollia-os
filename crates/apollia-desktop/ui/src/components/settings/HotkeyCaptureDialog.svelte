<script lang="ts">
  import { onDestroy, tick } from "svelte";
  import { fade } from "svelte/transition";
  import { t } from "svelte-i18n";
  import { Button } from "$lib/components/ui/button";
  import {
    parseHotkeyEvent,
    isValidHotkey,
    detectCollision,
    hotkeyChips,
    detectPlatform,
    type ParsedHotkey,
  } from "$lib/keyboard/hotkeyCapture";
  import KeyChip from "./KeyChip.svelte";

  interface Props {
    open: boolean;
    onconfirm: (combo: string) => void;
    oncancel: (reason?: "escape" | "timeout" | "close") => void;
    timeoutMs?: number;
  }

  let { open, onconfirm, oncancel, timeoutMs = 15000 }: Props = $props();

  let captured = $state<ParsedHotkey>({ modifiers: [], key: null, combo: "" });
  let announce = $state("");
  let remainingMs = $state(timeoutMs);
  let countdownId: ReturnType<typeof setInterval> | null = null;
  let timeoutId: ReturnType<typeof setTimeout> | null = null;
  let dialogRef = $state<HTMLDivElement | null>(null);
  let previouslyFocused: HTMLElement | null = null;

  const platform = detectPlatform();
  const valid = $derived(isValidHotkey(captured));
  const collision = $derived(valid && detectCollision(captured.combo));
  const chips = $derived(hotkeyChips(captured, platform));
  const remainingSec = $derived(Math.ceil(remainingMs / 1000));

  function clearTimers() {
    if (countdownId) { clearInterval(countdownId); countdownId = null; }
    if (timeoutId) { clearTimeout(timeoutId); timeoutId = null; }
  }

  function resetState() {
    captured = { modifiers: [], key: null, combo: "" };
    announce = "";
    remainingMs = timeoutMs;
  }

  function onKeydown(event: KeyboardEvent) {
    if (!open) return;
    event.preventDefault();
    event.stopPropagation();

    if (event.key === "Escape") {
      cancel("escape");
      return;
    }
    if (event.key === "Enter" && valid) {
      confirm();
      return;
    }

    const parsed = parseHotkeyEvent(event);
    captured = parsed;
    if (parsed.combo) {
      announce = $t("settings.stt.hotkey.captured", { values: { combo: chipsFromParsed(parsed).join(" ") } });
    }
  }

  function chipsFromParsed(parsed: ParsedHotkey): string[] {
    return hotkeyChips(parsed, platform);
  }

  function confirm() {
    if (!valid) return;
    const combo = captured.combo;
    clearTimers();
    onconfirm(combo);
  }

  function cancel(reason: "escape" | "timeout" | "close") {
    clearTimers();
    oncancel(reason);
  }

  function onBackdropClick(event: MouseEvent) {
    if (event.target === event.currentTarget) cancel("close");
  }

  $effect(() => {
    if (open) {
      resetState();
      previouslyFocused = document.activeElement as HTMLElement | null;

      window.addEventListener("keydown", onKeydown, { capture: true });

      countdownId = setInterval(() => {
        remainingMs = Math.max(0, remainingMs - 1000);
      }, 1000);
      timeoutId = setTimeout(() => cancel("timeout"), timeoutMs);

      tick().then(() => dialogRef?.focus());
    } else {
      window.removeEventListener("keydown", onKeydown, { capture: true });
      clearTimers();
      previouslyFocused?.focus();
      previouslyFocused = null;
    }
  });

  onDestroy(() => {
    window.removeEventListener("keydown", onKeydown, { capture: true });
    clearTimers();
  });
</script>

{#if open}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="fixed inset-0 flex items-center justify-center backdrop-blur-md bg-background/70"
    style="z-index: var(--z-overlay, 50);"
    role="dialog"
    aria-modal="true"
    aria-labelledby="hotkey-capture-title"
    tabindex="-1"
    transition:fade={{ duration: 150 }}
    onclick={onBackdropClick}
    data-testid="hotkey-capture-dialog"
  >
    <div
      bind:this={dialogRef}
      tabindex="-1"
      class="relative mx-4 w-full max-w-xl rounded-2xl border border-border bg-card p-10 text-card-foreground shadow-2xl outline-none"
      onclick={(e) => e.stopPropagation()}
    >
      <h2
        id="hotkey-capture-title"
        class="text-center text-xl font-semibold tracking-tight"
      >
        {$t("settings.stt.hotkey.capture_title")}
      </h2>
      <p class="mt-2 text-center text-sm text-muted-foreground">
        {$t("settings.stt.hotkey.capture_subtitle")}
      </p>

      <div class="my-10 flex min-h-[4.5rem] flex-wrap items-center justify-center gap-2" data-testid="hotkey-capture-chips">
        {#if chips.length === 0}
          <span class="animate-pulse text-2xl font-mono text-muted-foreground/70">
            {$t("settings.stt.hotkey.waiting")}
          </span>
        {:else}
          {#each chips as chip, i (i + chip)}
            <KeyChip label={chip} variant={i < chips.length - 1 ? "modifier" : "key"} />
          {/each}
        {/if}
      </div>

      {#if !valid && captured.combo}
        <p class="mb-4 text-center text-sm text-amber-500" data-testid="hotkey-capture-needs-modifier">
          {$t("settings.stt.hotkey.needs_modifier")}
        </p>
      {/if}

      {#if collision}
        <p
          class="mb-4 rounded-md border border-amber-500/40 bg-amber-500/10 px-3 py-2 text-center text-sm text-amber-600 dark:text-amber-400"
          data-testid="hotkey-capture-collision"
        >
          {$t("settings.stt.hotkey.collision_warning", { values: { combo: captured.combo } })}
        </p>
      {/if}

      <div class="flex items-center justify-between gap-4">
        <span class="text-xs text-muted-foreground" data-testid="hotkey-capture-countdown">
          {$t("settings.stt.hotkey.timeout_countdown", { values: { seconds: remainingSec } })}
        </span>
        <div class="flex gap-2">
          <Button variant="outline" onclick={() => cancel("close")} data-testid="hotkey-capture-cancel">
            {$t("settings.stt.hotkey.cancel")}
          </Button>
          <Button
            variant="default"
            onclick={confirm}
            disabled={!valid}
            data-testid="hotkey-capture-confirm"
          >
            {$t("settings.stt.hotkey.confirm")}
          </Button>
        </div>
      </div>

      <div class="sr-only" role="status" aria-live="polite" data-testid="hotkey-capture-announce">
        {announce}
      </div>
    </div>
  </div>
{/if}
