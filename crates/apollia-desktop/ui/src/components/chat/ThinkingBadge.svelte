<script lang="ts">
  import { t } from "svelte-i18n";
  import { BrainCircuit } from "lucide-svelte";

  interface Props {
    /** i18n key for the label. Defaults to `chat.status.thinking`. */
    labelKey?: string;
  }

  let { labelKey = "chat.status.thinking" }: Props = $props();
</script>

<span
  class="thinking-badge"
  role="status"
  aria-live="polite"
  data-testid="thinking-badge"
>
  <BrainCircuit size={11} class="text-primary/60" />
  <span class="label">{$t(labelKey)}</span>
  <span class="dots" aria-hidden="true">
    <span class="dot dot-1"></span>
    <span class="dot dot-2"></span>
    <span class="dot dot-3"></span>
  </span>
</span>

<style>
  .thinking-badge {
    display: inline-flex;
    align-items: center;
    gap: 0.375rem;
    padding: 0.125rem 0.5rem;
    border-radius: 9999px;
    background-color: hsl(var(--primary) / 0.08);
    color: hsl(var(--primary));
    font-size: 11px;
    font-weight: 500;
    line-height: 1.4;
  }

  .label {
    white-space: nowrap;
  }

  .dots {
    display: inline-flex;
    align-items: center;
    gap: 2px;
  }

  .dot {
    width: 3px;
    height: 3px;
    border-radius: 9999px;
    background-color: currentColor;
    opacity: 0.35;
    animation: thinking-dot 1.2s ease-in-out infinite;
  }

  .dot-1 {
    animation-delay: 0s;
  }
  .dot-2 {
    animation-delay: 0.2s;
  }
  .dot-3 {
    animation-delay: 0.4s;
  }

  @keyframes thinking-dot {
    0%,
    80%,
    100% {
      opacity: 0.35;
      transform: translateY(0);
    }
    40% {
      opacity: 1;
      transform: translateY(-1px);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .dot {
      animation: none;
      opacity: 0.7;
    }
  }
</style>
