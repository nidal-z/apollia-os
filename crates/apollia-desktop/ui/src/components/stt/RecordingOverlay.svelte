<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";

  let isRecording = $state(true);
  let hotkey = $state("…");

  // Voice visualizer - a scrolling waveform driven by the real capture level.
  // The Rust cpal stream (the same one the STT engine transcribes) emits a
  // `stt-audio-level` event ~30x/sec with a 0..1 peak amplitude; each event
  // pushes one new column so the bars move with actual input and sit flat on
  // silence. 80 bars with space-between fill the full window width at ~2px each.
  const BAR_COUNT = 80;
  const IDLE = 0.06;
  let bars = $state<number[]>(Array(BAR_COUNT).fill(IDLE));
  // Newest level lives at the right edge; older levels scroll left.
  let history = Array(BAR_COUNT).fill(IDLE);

  // Last-resort fallback: only animate synthetically if no real level event
  // has arrived shortly after recording starts (e.g. an older backend build
  // that does not emit levels). A real event cancels it immediately.
  let fallbackFrameId: number | null = null;
  let fallbackTimer: ReturnType<typeof setTimeout> | null = null;
  let receivedLevel = false;

  function pushLevel(level: number) {
    receivedLevel = true;
    cancelFallback();
    // Light gain so normal speech fills the bars; clamp to [IDLE, 1].
    const v = Math.max(IDLE, Math.min(1, level * 1.8));
    history = [...history.slice(1), v];
    bars = history;
  }

  function startFallbackWatch() {
    receivedLevel = false;
    cancelFallback();
    fallbackTimer = setTimeout(() => {
      if (!receivedLevel) animateFallback();
    }, 600);
  }

  function animateFallback() {
    let phase = 0;
    const tick = () => {
      phase += 0.06;
      bars = Array.from({ length: BAR_COUNT }, (_, i) =>
        Math.max(IDLE, 0.42 + 0.36 * Math.sin(phase + i * 0.35))
      );
      fallbackFrameId = requestAnimationFrame(tick);
    };
    fallbackFrameId = requestAnimationFrame(tick);
  }

  function cancelFallback() {
    if (fallbackTimer !== null) {
      clearTimeout(fallbackTimer);
      fallbackTimer = null;
    }
    if (fallbackFrameId !== null) {
      cancelAnimationFrame(fallbackFrameId);
      fallbackFrameId = null;
    }
  }

  function resetVisualizer() {
    cancelFallback();
    history = Array(BAR_COUNT).fill(IDLE);
    bars = history;
  }

  onMount(() => {
    const unlisteners = [
      listen<string>("stt-overlay-config", (event) => {
        hotkey = formatHotkey(event.payload);
      }),
      listen<number>("stt-audio-level", (event) => {
        if (isRecording) pushLevel(event.payload);
      }),
      listen("stt-recording-started", () => {
        isRecording = true;
        resetVisualizer();
        startFallbackWatch();
      }),
      listen("stt-recording-stopped", () => {
        isRecording = false;
        resetVisualizer();
      }),
    ];

    if (isRecording) startFallbackWatch();

    return () => {
      cancelFallback();
      for (const p of unlisteners) void p.then((fn) => fn());
    };
  });

  function formatHotkey(raw: string): string {
    return raw
      .split("+")
      .map((part) => {
        const trimmed = part.trim().toLowerCase();
        if (trimmed === "ctrl" || trimmed === "control") return "Ctrl";
        if (trimmed === "shift") return "Shift";
        if (trimmed === "alt" || trimmed === "option") return "Alt";
        if (trimmed === "meta" || trimmed === "cmd" || trimmed === "command") return "⌘";
        if (trimmed === "space") return "Space";
        return trimmed.charAt(0).toUpperCase() + trimmed.slice(1);
      })
      .join("+");
  }
</script>

{#if isRecording}
  <div class="overlay" role="status" aria-live="polite">
    <div class="visualizer" aria-hidden="true">
      {#each bars as h}
        <span class="bar" style="height: {Math.round(h * 100)}%"></span>
      {/each}
    </div>
    <span class="hint">
      {$t("chat.recording_hotkey_hint", { values: { hotkey } })}
    </span>
  </div>
{/if}

<style>
  /* ── Theming tokens ───────────────────────────── */
  .overlay {
    --bg:     rgba(16, 16, 20, 0.92);
    --hint:   rgba(255, 255, 255, 0.35);
    --bar:    hsl(240 91% 65%);
    --radius: 14px;
  }

  @media (prefers-color-scheme: light) {
    .overlay {
      --bg:   rgba(248, 248, 252, 0.94);
      --hint: rgba(0, 0, 0, 0.35);
      --bar:  hsl(240 91% 50%);
    }
  }

  /* ── Container - fills the transparent Tauri window ── */
  .overlay {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0;
    padding: 16px 16px 13px;
    box-sizing: border-box;
    width: 100%;
    height: 100%;
    background: var(--bg);
    border-radius: var(--radius);
    font-family: -apple-system, BlinkMacSystemFont, sans-serif;
    user-select: none;
    -webkit-user-select: none;
  }

  /* ── Visualizer - fixed height so bar % heights resolve ── */
  .visualizer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    height: 54px;
    flex-shrink: 0;
  }

  /* 80 bars × 2px width + space-between gaps ≈ full width */
  .bar {
    width: 2px;
    flex-shrink: 0;
    border-radius: 2px;
    background: var(--bar);
    height: 6%;
    min-height: 3px;
    max-height: 100%;
    transition: height 55ms ease-out;
  }

  /* ── Hint ─────────────────────────────────────── */
  .hint {
    font-size: 10px;
    color: var(--hint);
    letter-spacing: 0.01em;
    margin-top: 8px;
    white-space: nowrap;
    flex-shrink: 0;
  }

</style>
