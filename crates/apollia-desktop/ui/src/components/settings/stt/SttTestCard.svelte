<script lang="ts">
  /**
   * SttTestCard - the live dictation self-test, the one expressive focal of the
   * STT settings page.
   *
   * Start records through the throwaway tour recorder; the real cpal capture
   * stream broadcasts `stt-audio-level` (0..1 peak) ~30x/sec, which drives the
   * scrolling vu-meter so the bars move with actual input and sit flat on
   * silence. The Rust pipeline then broadcasts `stt-transcribed` with the text.
   * The signature gradient is reserved for the live bars; the rest stays calm.
   */
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { Mic, Play, Square, RefreshCw, Check, AlertTriangle } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { addToast } from "$lib/components/ui/toast";
  import { ensureMicPermission } from "$lib/stt/micPermission";
  import {
    DICTATION_FAILED_EVENT,
    failureMessageKey,
    readFailureReason,
  } from "$lib/stt/dictationFailure";
  import { startTourRecording, stopTourRecording } from "$lib/ipc/stt";

  interface Props {
    /** Silence threshold in dB (-80..0); positions the meter guide line. */
    silenceThresholdDb: number;
    disabled?: boolean;
  }

  let { silenceThresholdDb, disabled = false }: Props = $props();

  const BAR_COUNT = 44;
  const IDLE = 0.08;

  let recording = $state(false);
  let busy = $state(false);
  let result = $state<string | null>(null);
  // Why the last test produced no text. Without it a self-test on a muted
  // microphone leaves the card spinning on "transcribing" with nothing to say.
  let failure = $state<string | null>(null);
  let bars = $state<number[]>(Array(BAR_COUNT).fill(IDLE));
  let history = Array(BAR_COUNT).fill(IDLE);

  // Guide line height, measured from the meter floor: -80 dB floor, 0 dB ceiling.
  const thresholdPercent = $derived(
    Math.min(100, Math.max(0, ((silenceThresholdDb + 80) / 80) * 100)),
  );

  function pushLevel(level: number): void {
    const v = Math.max(IDLE, Math.min(1, level * 1.8));
    history = [...history.slice(1), v];
    bars = history;
  }

  function resetMeter(): void {
    history = Array(BAR_COUNT).fill(IDLE);
    bars = history;
  }

  async function toggle(): Promise<void> {
    if (busy) return;
    busy = true;
    try {
      if (recording) {
        await stopTourRecording();
        // busy stays true until the transcription event arrives.
      } else {
        result = null;
        failure = null;
        resetMeter();
        await ensureMicPermission();
        await startTourRecording();
        recording = true;
        busy = false;
      }
    } catch (err) {
      recording = false;
      busy = false;
      addToast(err instanceof Error ? err.message : String(err), "error");
    }
  }

  onMount(() => {
    let cancelled = false;
    const unlisteners: Array<() => void> = [];
    void listen<number>("stt-audio-level", (event) => {
      if (recording) pushLevel(event.payload);
    }).then((fn) => (cancelled ? fn() : unlisteners.push(fn)));
    void listen<{ text?: string } | string>("stt-transcribed", (event) => {
      if (!recording) return;
      result =
        typeof event.payload === "string" ? event.payload : (event.payload?.text ?? "");
      failure = null;
      recording = false;
      busy = false;
      resetMeter();
    }).then((fn) => (cancelled ? fn() : unlisteners.push(fn)));
    void listen(DICTATION_FAILED_EVENT, (event) => {
      if (!recording && !busy) return;
      failure = $t(failureMessageKey(readFailureReason(event.payload)));
      result = null;
      recording = false;
      busy = false;
      resetMeter();
    }).then((fn) => (cancelled ? fn() : unlisteners.push(fn)));
    return () => {
      cancelled = true;
      for (const fn of unlisteners) fn();
    };
  });
</script>

<div
  class="flex flex-col gap-3 rounded-lg border border-border bg-surface-2/45 p-4"
  data-testid="stt-test-card"
>
  <div class="flex items-center justify-between gap-3">
    <div class="flex items-center gap-2.5">
      <span
        class="flex h-8 w-8 shrink-0 items-center justify-center rounded-full transition-colors {recording
          ? 'bg-destructive/10 text-destructive ring-2 ring-destructive/20 motion-safe:animate-pulse'
          : 'bg-primary/10 text-primary'}"
        aria-hidden="true"
      >
        <Mic size={15} strokeWidth={1.75} />
      </span>
      <div class="min-w-0">
        <div class="text-label-md text-foreground">
          {recording ? $t("settings.stt.test.listening") : $t("settings.stt.test.label")}
        </div>
        <div class="text-caption text-muted-foreground">{$t("settings.stt.test.hint")}</div>
      </div>
    </div>
    <Button
      variant={recording ? "destructive" : "default"}
      size="sm"
      loading={busy && !recording}
      disabled={disabled || (busy && !recording)}
      onclick={toggle}
      data-testid={recording ? "stt-test-stop" : "stt-test-start"}
    >
      {#snippet icon()}
        {#if recording}
          <Square size={13} strokeWidth={2} />
        {:else if !busy}
          {#if result !== null}
            <RefreshCw size={13} strokeWidth={1.75} />
          {:else}
            <Play size={13} strokeWidth={1.75} />
          {/if}
        {/if}
      {/snippet}
      {#if recording}
        {$t("settings.stt.test.stop")}
      {:else if busy}
        {$t("settings.stt.test.transcribing")}
      {:else if result !== null}
        {$t("settings.stt.test.redo")}
      {:else}
        {$t("settings.stt.test.start")}
      {/if}
    </Button>
  </div>

  <div
    class="relative flex h-20 items-center overflow-hidden rounded-md border border-border/60 bg-surface-1 px-3"
    aria-hidden="true"
  >
    <div class="flex h-12 w-full items-center gap-0.5">
      {#each bars as h, i (i)}
        <span
          class="min-w-0 flex-1 rounded-full transition-[height] duration-75 {recording
            ? 'bg-primary-gradient'
            : 'bg-muted-foreground/40'}"
          style="height: {Math.round(h * 100)}%"
        ></span>
      {/each}
    </div>

    {#if recording}
      <div
        class="pointer-events-none absolute inset-x-0 border-t border-dashed border-warning/70"
        style="bottom: {thresholdPercent}%"
      >
        <span
          class="absolute right-2 -top-2 rounded-sm bg-surface-1 px-1 text-overline text-warning"
        >
          {$t("settings.stt.test.threshold_label", { values: { db: silenceThresholdDb } })}
        </span>
      </div>
    {:else if result === null && failure === null}
      <div class="absolute inset-0 grid place-items-center text-caption text-muted-foreground">
        {$t("settings.stt.test.idle_hint")}
      </div>
    {/if}
  </div>

  {#if failure !== null}
    <div
      class="flex items-start gap-2 rounded-md border border-warning/30 bg-warning/10 px-3 py-2 text-body-sm text-warning"
      role="alert"
      data-testid="stt-test-failure"
    >
      <AlertTriangle size={13} strokeWidth={2} class="mt-0.5 shrink-0" />
      <span>{failure}</span>
    </div>
  {/if}

  {#if result !== null}
    <div
      class="rounded-md border border-border/60 bg-card px-3 py-2"
      data-testid="stt-test-result"
    >
      <div class="mb-0.5 flex items-center gap-1.5 text-overline uppercase text-success">
        <Check size={12} strokeWidth={2.2} />
        {$t("settings.stt.test.result_label")}
      </div>
      <p class="text-body-sm text-foreground">
        {result || $t("settings.stt.test.empty")}
      </p>
    </div>
  {/if}
</div>
