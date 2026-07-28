<script lang="ts">
  /**
   * AgentListSection - a titled sidebar section wrapping an accessible listbox.
   *
   * Renders the uppercase section eyebrow with its count, then a
   * `role="listbox"` container with roving-tabindex keyboard navigation
   * (Up / Down / Home / End) over the rows inside. Rows opt in via the
   * `[data-nav-row]` attribute on their main control.
   */
  import type { Snippet } from "svelte";
  import { listNavigation } from "$lib/components/operator/listNavigation";

  interface Props {
    title: string;
    count: number;
    children: Snippet;
    class?: string;
  }

  let { title, count, children, class: className = "" }: Props = $props();
</script>

<div class={className}>
  <div
    class="mb-1.5 px-2 text-overline uppercase text-muted-foreground/80"
  >
    {title} · {count}
  </div>
  <div
    role="listbox"
    aria-label={title}
    use:listNavigation={{ rowSelector: "[data-nav-row]" }}
  >
    {@render children()}
  </div>
</div>
