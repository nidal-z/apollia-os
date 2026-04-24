<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";

  let isRecording = $state(true);
  let hotkey = $state("…");

  // Voice visualizer — time-domain waveform split into segments.
  // 80 bars with flex:1 fill the full window width at ~2px each.
  const BAR_COUNT = 80;
  let bars = $state<number[]>(Array(BAR_COUNT).fill(0.08));
  let smoothed = Array(BAR_COUNT).fill(0.08);

  let animFrameId: number | null = null;
  let analyser: AnalyserNode | null = null;
  let audioCtx: AudioContext | null = null;
  let stream: MediaStream | null = null;

  async function startVisualizer() {
    // Start fallback immediately so bars are always animated.
    // getUserMedia can hang forever in Tauri overlay windows — never reaching the catch.
    animateFallback();

    try {
      const result = await Promise.race([
        navigator.mediaDevices.getUserMedia({ audio: true }),
        new Promise<never>((_, reject) =>
          setTimeout(() => reject(new Error("mic_timeout")), 800)
        ),
      ]);
      // Mic access granted — cancel fallback and switch to real audio input.
      if (animFrameId !== null) {
        cancelAnimationFrame(animFrameId);
        animFrameId = null;
      }
      stream = result;
      audioCtx = new AudioContext();
      analyser = audioCtx.createAnalyser();
      analyser.fftSize = 2048;
      analyser.smoothingTimeConstant = 0;
      audioCtx.createMediaStreamSource(stream).connect(analyser);
      tickVisualizer();
    } catch {
      // Fallback animation is already running — nothing to do.
    }
  }

  function tickVisualizer() {
    if (!analyser) return;

    const data = new Uint8Array(analyser.fftSize);
    analyser.getByteTimeDomainData(data); // waveform, center=128

    const segSize = Math.floor(data.length / BAR_COUNT);
    for (let i = 0; i < BAR_COUNT; i++) {
      let peak = 0;
      for (let j = i * segSize; j < (i + 1) * segSize; j++) {
        peak = Math.max(peak, Math.abs(data[j] - 128));
      }
      // 2.5× gain so normal speech fills the bars visually
      const raw = Math.max(0.06, Math.min(1.0, (peak / 128) * 2.5));
      // Fast attack, slow decay
      const alpha = raw > smoothed[i] ? 0.65 : 0.12;
      smoothed[i] = smoothed[i] * (1 - alpha) + raw * alpha;
    }

    bars = [...smoothed];
    animFrameId = requestAnimationFrame(tickVisualizer);
  }

  function animateFallback() {
    let phase = 0;
    const tick = () => {
      phase += 0.06;
      bars = Array.from({ length: BAR_COUNT }, (_, i) =>
        Math.max(0.06, 0.42 + 0.36 * Math.sin(phase + i * 0.35))
      );
      animFrameId = requestAnimationFrame(tick);
    };
    animFrameId = requestAnimationFrame(tick);
  }

  function stopVisualizer() {
    if (animFrameId !== null) {
      cancelAnimationFrame(animFrameId);
      animFrameId = null;
    }
    stream?.getTracks().forEach((t) => t.stop());
    stream = null;
    void audioCtx?.close();
    audioCtx = null;
    analyser = null;
    smoothed = Array(BAR_COUNT).fill(0.08);
    bars = Array(BAR_COUNT).fill(0.08);
  }

  onMount(() => {
    const unlisteners = [
      listen<string>("stt-overlay-config", (event) => {
        hotkey = formatHotkey(event.payload);
      }),
      listen("stt-recording-started", () => {
        isRecording = true;
        void startVisualizer();
      }),
      listen("stt-recording-stopped", () => {
        isRecording = false;
        stopVisualizer();
      }),
    ];

    if (isRecording) void startVisualizer();

    return () => {
      stopVisualizer();
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

  /* ── Container — fills the transparent Tauri window ── */
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
    font-family: -apple-system, BlinkMacSystemFont, "Inter", sans-serif;
    user-select: none;
    -webkit-user-select: none;
  }

  /* ── Visualizer — fixed height so bar % heights resolve ── */
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
