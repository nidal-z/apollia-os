<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";

  let hotkey = $state("...");

  onMount(() => {
    const unlisten = listen<string>("stt-overlay-config", (event) => {
      hotkey = formatHotkey(event.payload);
    });
    return () => {
      unlisten.then((fn) => fn());
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
        if (trimmed === "meta" || trimmed === "cmd" || trimmed === "command")
          return "Cmd";
        if (trimmed === "space") return "Space";
        return trimmed.charAt(0).toUpperCase() + trimmed.slice(1);
      })
      .join(" + ");
  }
</script>

<div class="overlay" role="status" aria-live="polite">
  <span class="indicator" aria-hidden="true"></span>
  <div class="text">
    <span class="title">Enregistrement en cours...</span>
    <span class="hint">{hotkey} pour arr&ecirc;ter</span>
  </div>
</div>

<style>
  .overlay {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 20px;
    background: rgba(0, 0, 0, 0.85);
    border-radius: 12px;
    color: white;
    font-family: -apple-system, BlinkMacSystemFont, "Inter", sans-serif;
    user-select: none;
    -webkit-user-select: none;
  }

  .indicator {
    display: inline-block;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: #ef4444;
    flex-shrink: 0;
    animation: pulse 1.5s ease-in-out infinite;
  }

  @keyframes pulse {
    0%,
    100% {
      opacity: 1;
      transform: scale(1);
    }
    50% {
      opacity: 0.4;
      transform: scale(0.85);
    }
  }

  .text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .title {
    font-size: 14px;
    font-weight: 600;
  }

  .hint {
    font-size: 11px;
    opacity: 0.7;
  }
</style>
