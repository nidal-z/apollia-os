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

<section class="overflow-hidden rounded-xl border border-border bg-card shadow-sm {className}">
  <div class="border-b border-border/60 px-4 py-2.5 text-overline uppercase text-muted-foreground">
    {title} · {count}
  </div>
  <div
    class="p-1.5"
    role="listbox"
    aria-label={title}
    use:listNavigation={{ rowSelector: "[data-nav-row]" }}
  >
    {@render children()}
  </div>
</section>
