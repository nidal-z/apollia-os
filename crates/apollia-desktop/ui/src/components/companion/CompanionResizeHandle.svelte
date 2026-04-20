<script lang="ts">
  import { t } from "svelte-i18n";

  interface Props {
    onpointerdown?: (event: PointerEvent) => void;
  }

  let { onpointerdown }: Props = $props();
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="group absolute bottom-0 right-0 h-4 w-4 opacity-0 transition-opacity hover:opacity-50"
  style:cursor="nwse-resize"
  {onpointerdown}
  role="separator"
  aria-label={$t("companion.drag.resize_aria")}
  data-testid="companion-resize-handle"
>
  <svg viewBox="0 0 16 16" class="h-full w-full text-foreground">
    <path
      d="M11 11L15 15M7 11L15 15M11 7L15 15"
      stroke="currentColor"
      stroke-width="1.5"
      stroke-linecap="round"
      fill="none"
    />
  </svg>
</div>
