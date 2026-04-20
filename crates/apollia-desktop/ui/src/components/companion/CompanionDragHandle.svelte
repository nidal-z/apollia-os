<script lang="ts">
  import { t } from "svelte-i18n";
  import { GripVertical } from "lucide-svelte";

  interface Props {
    onpointerdown?: (event: PointerEvent) => void;
  }

  let { onpointerdown }: Props = $props();
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<span
  class="group inline-flex h-6 w-4 shrink-0 cursor-grab items-center justify-center text-muted-foreground active:cursor-grabbing"
  {onpointerdown}
  role="separator"
  aria-orientation="vertical"
  aria-label={$t("companion.drag.handle_aria")}
  data-testid="companion-drag-handle"
>
  <GripVertical
    size={14}
    strokeWidth={1.75}
    class="opacity-0 transition-opacity group-hover:opacity-70"
  />
</span>
