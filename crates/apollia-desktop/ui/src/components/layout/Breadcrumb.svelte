<script lang="ts">
  /**
   * Topbar breadcrumb — derives the trail from `currentRoute` via
   * `routeMeta` and delegates rendering to the shared `Breadcrumbs`
   * primitive (US-SP42-018).
   */
  import { t } from "svelte-i18n";
  import { currentRoute, navigateTo, type Route } from "$lib/stores/navigation";
  import { buildTrail, routeMeta } from "$lib/navigation/routeMeta";
  import { Breadcrumbs } from "$lib/components/ui/breadcrumbs";
  import type { BreadcrumbItem } from "$lib/components/ui/breadcrumbs";

  const items = $derived.by<BreadcrumbItem[]>(() => {
    const trail = buildTrail($currentRoute);
    return trail.map((r: Route, i: number) => {
      const meta = routeMeta[r];
      const isLast = i === trail.length - 1;
      return {
        label: $t(meta.labelKey),
        icon: meta.icon,
        // Parent segments keep `href` so the primitive renders them as
        // anchors; click handler swallows navigation and routes via the
        // store. Last segment stays unlinked (aria-current="page").
        href: isLast ? undefined : `#${r}`,
      } satisfies BreadcrumbItem;
    });
  });

  function onClick(event: MouseEvent) {
    const target = (event.target as HTMLElement | null)?.closest("a");
    if (!target) return;
    const href = target.getAttribute("href") ?? "";
    if (!href.startsWith("#")) return;
    event.preventDefault();
    const route = href.slice(1) as Route;
    navigateTo(route);
  }
</script>

<div
  class="min-w-0 flex-1 truncate"
  onclick={onClick}
  onkeydown={(e) => {
    if (e.key === "Enter" || e.key === " ") onClick(e as unknown as MouseEvent);
  }}
  role="presentation"
  data-testid="topbar-breadcrumb"
>
  <Breadcrumbs {items} />
</div>
