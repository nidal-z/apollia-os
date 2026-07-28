<script lang="ts">
  import { t } from "svelte-i18n";
  import { GripVertical } from "lucide-svelte";

  interface Props {
    /** Pointer-drag start, forwarded from the toolbar drag zone. */
    onpointerdown?: (event: PointerEvent) => void;
    /** Keyboard nudge in viewport pixels (arrow keys). */
    onmove?: (dx: number, dy: number) => void;
  }

  let { onpointerdown, onmove }: Props = $props();

  const STEP = 20;

  function handleKeydown(event: KeyboardEvent) {
    let dx = 0;
    let dy = 0;
    switch (event.key) {
      case "ArrowLeft":
        dx = -STEP;
        break;
      case "ArrowRight":
        dx = STEP;
        break;
      case "ArrowUp":
        dy = -STEP;
        break;
      case "ArrowDown":
        dy = STEP;
        break;
      default:
        return;
    }
    event.preventDefault();
    onmove?.(dx, dy);
  }
</script>

<button
  type="button"
  class="group inline-flex h-6 w-4 shrink-0 cursor-grab items-center justify-center rounded-sm text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 active:cursor-grabbing"
  {onpointerdown}
  onkeydown={handleKeydown}
  aria-label={$t("companion.drag.handle_aria")}
  data-testid="companion-drag-handle"
>
  <GripVertical
    size={14}
    strokeWidth={1.75}
    class="opacity-0 transition-opacity group-hover:opacity-70 group-focus-visible:opacity-100"
  />
</button>
