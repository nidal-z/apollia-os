<script lang="ts">
  import { t } from "svelte-i18n";

  export type ConnectionSegment = "active" | "suggested" | "catalogue";

  interface Props {
    active: ConnectionSegment;
    activeCount: number;
    suggestedCount: number;
    catalogueCount: number;
    onchange: (segment: ConnectionSegment) => void;
  }

  let { active, activeCount, suggestedCount, catalogueCount, onchange }: Props = $props();

  const SEGMENTS: Array<{ id: ConnectionSegment; labelKey: string; count: () => number }> = [
    { id: "active", labelKey: "connections.hero.segment_active", count: () => activeCount },
    { id: "suggested", labelKey: "connections.hero.segment_suggested", count: () => suggestedCount },
    { id: "catalogue", labelKey: "connections.hero.segment_catalogue", count: () => catalogueCount },
  ];

  let tablistRef = $state<HTMLDivElement | null>(null);

  function handleKeydown(event: KeyboardEvent) {
    if (event.key !== "ArrowRight" && event.key !== "ArrowLeft" && event.key !== "Home" && event.key !== "End") return;
    event.preventDefault();
    const idx = SEGMENTS.findIndex((s) => s.id === active);
    let next = idx;
    if (event.key === "ArrowRight") next = (idx + 1) % SEGMENTS.length;
    else if (event.key === "ArrowLeft") next = (idx - 1 + SEGMENTS.length) % SEGMENTS.length;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = SEGMENTS.length - 1;
    onchange(SEGMENTS[next].id);
    requestAnimationFrame(() => {
      const btn = tablistRef?.querySelector<HTMLButtonElement>(`[data-segment="${SEGMENTS[next].id}"]`);
      btn?.focus();
    });
  }
</script>

<div class="flex flex-col gap-5" data-testid="connections-hero">
  <div>
    <h1 class="text-2xl font-semibold text-foreground">{$t("connections.hero.title")}</h1>
    <p class="mt-1 text-sm text-muted-foreground">{$t("connections.hero.subtitle")}</p>
  </div>

  <div
    bind:this={tablistRef}
    class="flex items-center gap-1 rounded-lg border border-border bg-muted/40 p-1 w-fit"
    role="tablist"
    tabindex="-1"
    aria-label={$t("connections.hero.tablist_label")}
    onkeydown={handleKeydown}
  >
    {#each SEGMENTS as seg}
      {@const isActive = seg.id === active}
      <button
        type="button"
        role="tab"
        tabindex={isActive ? 0 : -1}
        aria-selected={isActive}
        aria-current={isActive ? "page" : undefined}
        data-segment={seg.id}
        data-testid="connections-segment-{seg.id}"
        class="relative inline-flex items-center gap-2 rounded-md px-3 py-1.5 text-sm font-medium transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-primary/60 {isActive
          ? 'bg-card text-foreground shadow-sm'
          : 'text-muted-foreground hover:text-foreground'}"
        onclick={() => onchange(seg.id)}
      >
        <span>{$t(seg.labelKey)}</span>
        <span
          class="inline-flex h-5 min-w-[20px] items-center justify-center rounded-full px-1.5 text-[10px] font-semibold {isActive
            ? 'bg-primary/10 text-primary'
            : 'bg-muted text-muted-foreground'}"
          aria-hidden="true"
        >
          {seg.count()}
        </span>
      </button>
    {/each}
  </div>
</div>
