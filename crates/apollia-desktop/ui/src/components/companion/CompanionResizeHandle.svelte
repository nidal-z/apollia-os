<script lang="ts">
  import { t } from "svelte-i18n";

  interface Props {
    /** Pointer-drag start for freeform resize. */
    onpointerdown?: (event: PointerEvent) => void;
    /** Keyboard resize step in pixels (arrow keys). */
    onresize?: (dw: number, dh: number) => void;
  }

  let { onpointerdown, onresize }: Props = $props();

  const STEP = 20;

  function handleKeydown(event: KeyboardEvent) {
    let dw = 0;
    let dh = 0;
    switch (event.key) {
      case "ArrowLeft":
        dw = -STEP;
        break;
      case "ArrowRight":
        dw = STEP;
        break;
      case "ArrowUp":
        dh = -STEP;
        break;
      case "ArrowDown":
        dh = STEP;
        break;
      default:
        return;
    }
    event.preventDefault();
    onresize?.(dw, dh);
  }
</script>

<button
  type="button"
  class="group absolute bottom-0 right-0 h-4 w-4 cursor-nwse-resize rounded-sm text-foreground opacity-0 transition-opacity hover:opacity-50 focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
  {onpointerdown}
  onkeydown={handleKeydown}
  aria-label={$t("companion.drag.resize_aria")}
  data-testid="companion-resize-handle"
>
  <svg viewBox="0 0 16 16" class="h-full w-full" aria-hidden="true">
    <path
      d="M11 11L15 15M7 11L15 15M11 7L15 15"
      stroke="currentColor"
      stroke-width="1.5"
      stroke-linecap="round"
      fill="none"
    />
  </svg>
</button>
