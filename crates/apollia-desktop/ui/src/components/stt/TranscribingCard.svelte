<script lang="ts">
  /**
   * TranscribingCard - focal "recording in progress" state.
   *
   * Shown while the microphone is live (`isRecording`). This is the one card
   * that carries expressive depth: a shimmering gradient wash and an animated
   * waveform. It shows only what the runtime actually reports (a live flag);
   * no fabricated percentage, ETA, or streamed partial text. Every animation
   * collapses under `prefers-reduced-motion`.
   */
  import { t } from "svelte-i18n";
  import { Mic } from "lucide-svelte";
  import { Card } from "$lib/components/ui/card";
  import TranscriptWaveform from "./TranscriptWaveform.svelte";
</script>

<Card
  surface="solid"
  class="relative overflow-hidden ring-1 ring-primary/30"
  data-testid="transcribing-card"
>
  <div class="live-shimmer pointer-events-none absolute inset-0" aria-hidden="true"></div>
  <div class="relative flex gap-3.5 p-4">
    <div
      class="flex size-8 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary"
    >
      <Mic size={16} strokeWidth={1.75} aria-hidden="true" />
    </div>
    <div class="min-w-0 flex-1">
      <div class="flex items-center gap-2">
        <span class="text-label-md font-medium text-foreground">
          {$t("transcriptions.recording")}
        </span>
        <span
          class="ml-auto inline-flex items-center gap-1.5 rounded-full bg-destructive/10 px-2 py-0.5 text-overline font-semibold text-destructive"
          role="status"
        >
          <span class="rec-dot size-1.5 rounded-full bg-current"></span>
          {$t("transcriptions.live")}
        </span>
      </div>
      <div class="mt-2.5">
        <TranscriptWaveform seed="live" live />
      </div>
    </div>
  </div>
</Card>

<style>
  .live-shimmer {
    background: linear-gradient(
      120deg,
      hsl(var(--primary) / 0.1),
      hsl(var(--secondary) / 0.05),
      hsl(var(--primary) / 0.1)
    );
    background-size: 200% 100%;
    animation: live-shimmer 2.6s linear infinite;
  }
  @keyframes live-shimmer {
    to {
      background-position: 200% 0;
    }
  }
  .rec-dot {
    animation: rec-pulse 1.4s ease-in-out infinite;
  }
  @keyframes rec-pulse {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.35;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .live-shimmer,
    .rec-dot {
      animation: none;
    }
  }
</style>
