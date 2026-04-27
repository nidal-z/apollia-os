<script lang="ts">
  import { t } from "svelte-i18n";
  import { Play, Pause, SkipBack, SkipForward } from "lucide-svelte";
  import type { ReplayState, SessionEvent } from "$lib/types";

  interface Props {
    events: SessionEvent[];
    state: ReplayState;
    /** Vitesses autorisées — fournies par : 1x / 2x / 5x. */
    onStateChange?: (next: ReplayState) => void;
    onSeek?: (cursor: number) => void;
  }

  let { events, state, onStateChange, onSeek }: Props = $props();

  const SPEEDS = [1, 2, 5] as const;

  function togglePlay() {
    onStateChange?.({ ...state, playing: !state.playing });
  }

  function stepBack() {
    const next = Math.max(0, state.cursor - 1);
    onSeek?.(next);
  }

  function stepForward() {
    const max = Math.max(0, events.length - 1);
    const next = Math.min(max, state.cursor + 1);
    onSeek?.(next);
  }

  function setSpeed(speed: number) {
    onStateChange?.({ ...state, speed });
  }
</script>

<div class="replay-controls" data-testid="session-replay-controls">
  <button
    type="button"
    class="icon-btn"
    aria-label={$t("session_meta.replay.back", { default: "Previous event" })}
    onclick={stepBack}
    disabled={state.cursor === 0}
  >
    <SkipBack size={14} />
  </button>

  <button
    type="button"
    class="icon-btn primary"
    aria-label={state.playing
      ? $t("session_meta.replay.pause", { default: "Pause" })
      : $t("session_meta.replay.play", { default: "Play" })}
    onclick={togglePlay}
    data-testid="session-replay-play"
  >
    {#if state.playing}
      <Pause size={14} />
    {:else}
      <Play size={14} />
    {/if}
  </button>

  <button
    type="button"
    class="icon-btn"
    aria-label={$t("session_meta.replay.forward", { default: "Next event" })}
    onclick={stepForward}
    disabled={state.cursor >= events.length - 1}
  >
    <SkipForward size={14} />
  </button>

  <span class="cursor-readout" aria-live="polite">
    {state.cursor + 1} / {Math.max(1, events.length)}
  </span>

  <div class="speeds" role="group" aria-label={$t("session_meta.replay.speed", { default: "Speed" })}>
    {#each SPEEDS as sp (sp)}
      <button
        type="button"
        class="speed-btn"
        class:active={state.speed === sp}
        onclick={() => setSpeed(sp)}
      >
        {sp}x
      </button>
    {/each}
  </div>
</div>

<style>
  .replay-controls {
    display: inline-flex;
    align-items: center;
    gap: 0.375rem;
    padding: 0.25rem 0.5rem;
    border-radius: 9999px;
    border: 1px solid hsl(var(--border));
    background-color: hsl(var(--card) / 0.6);
  }
  .icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border-radius: 9999px;
    border: none;
    background: transparent;
    color: hsl(var(--foreground));
    cursor: pointer;
    transition: background-color 120ms ease;
  }
  .icon-btn:hover:not(:disabled) {
    background-color: hsl(var(--secondary) / 0.6);
  }
  .icon-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .icon-btn.primary {
    background-color: hsl(var(--primary) / 0.15);
    color: hsl(var(--primary));
  }
  .cursor-readout {
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    color: hsl(var(--muted-foreground));
    min-width: 3.5rem;
    text-align: center;
  }
  .speeds {
    display: inline-flex;
    gap: 0.125rem;
    border-left: 1px solid hsl(var(--border));
    padding-left: 0.375rem;
    margin-left: 0.125rem;
  }
  .speed-btn {
    padding: 0.125rem 0.375rem;
    border-radius: 0.25rem;
    border: none;
    background: transparent;
    font-size: 11px;
    font-weight: 500;
    color: hsl(var(--muted-foreground));
    cursor: pointer;
  }
  .speed-btn:hover {
    color: hsl(var(--foreground));
  }
  .speed-btn.active {
    background-color: hsl(var(--primary) / 0.15);
    color: hsl(var(--primary));
  }
</style>
