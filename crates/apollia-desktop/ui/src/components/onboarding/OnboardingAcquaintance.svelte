<script lang="ts">
  import { Sparkles } from "lucide-svelte";
</script>

<!-- Full implementation delivered by STORY-419. -->
<div class="phase-placeholder" data-testid="onboarding-acquaintance">
  <div class="placeholder-logo">
    <Sparkles size={28} strokeWidth={1.5} class="text-white" />
  </div>
</div>

<style>
  .phase-placeholder {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: flex;
    align-items: center;
    justify-content: center;
    background: #FFF8F0;
  }

  .placeholder-logo {
    width: 3.5rem;
    height: 3.5rem;
    border-radius: 1rem;
    background: linear-gradient(135deg, #3435f5, #7c5fd6);
    display: flex;
    align-items: center;
    justify-content: center;
    animation: pulse 2s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.6; }
  }
</style>
