<script lang="ts">
  interface Props {
    /** Whether the STT engine is available. When false, nothing is rendered. */
    sttAvailable: boolean;
    /** Whether microphone recording is currently active. Controls pulse visibility. */
    isRecording: boolean;
  }

  const { sttAvailable, isRecording }: Props = $props();
</script>

{#if sttAvailable}
  <div
    class="voice-indicator"
    class:recording={isRecording}
    aria-hidden="true"
    data-testid="voice-indicator"
  >
    <span class="ring ring-1"></span>
    <span class="ring ring-2"></span>
    <span class="ring ring-3"></span>
    <svg
      class="mic-icon"
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-label="Microphone"
    >
      <path d="M12 2a3 3 0 0 1 3 3v7a3 3 0 0 1-6 0V5a3 3 0 0 1 3-3Z" />
      <path d="M19 10v2a7 7 0 0 1-14 0v-2" />
      <line x1="12" x2="12" y1="19" y2="22" />
    </svg>
  </div>
{/if}

<style>
  .voice-indicator {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 2rem;
    height: 2rem;
  }

  /* Concentric rings — invisible until recording starts. */
  .ring {
    position: absolute;
    inset: 0;
    border-radius: 50%;
    border: 1.5px solid rgba(52, 53, 245, 0.5);
    opacity: 0;
    transform: scale(1);
    pointer-events: none;
  }

  /* Stagger ring animations so they ripple outward. */
  .voice-indicator.recording .ring-1 {
    animation: ripple 1.4s ease-out infinite;
  }

  .voice-indicator.recording .ring-2 {
    animation: ripple 1.4s ease-out 0.35s infinite;
  }

  .voice-indicator.recording .ring-3 {
    animation: ripple 1.4s ease-out 0.7s infinite;
  }

  @keyframes ripple {
    0% {
      opacity: 0.7;
      transform: scale(1);
    }
    100% {
      opacity: 0;
      transform: scale(2.4);
    }
  }

  .mic-icon {
    position: relative;
    z-index: 1;
    width: 1rem;
    height: 1rem;
    color: #3435f5;
    transition: color 120ms ease;
  }

  .voice-indicator.recording .mic-icon {
    color: #e53e3e;
  }
</style>
