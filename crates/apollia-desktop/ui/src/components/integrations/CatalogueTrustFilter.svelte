<script module lang="ts">
  /** One of the four trust levels that can be toggled in the filter bar. */
  export type TrustFilterOption = "verified_official" | "community_verified" | "community" | "custom";
</script>

<script lang="ts">
  import { t } from "svelte-i18n";
  import { cn } from "$lib/utils";

  interface Props {
    selected: TrustFilterOption[];
    onchange: (selected: TrustFilterOption[]) => void;
  }

  let { selected, onchange }: Props = $props();

  const ALL_OPTIONS: TrustFilterOption[] = [
    "verified_official",
    "community_verified",
    "community",
    "custom",
  ];

  /**
   * Visual configuration for each chip.
   * Active classes mirror the Badge variant colors from TrustBadge for visual consistency.
   */
  const CHIP_CLASSES: Record<TrustFilterOption, { active: string; inactive: string }> = {
    verified_official: {
      active:
        "bg-emerald-50 text-emerald-700 border-emerald-300 dark:bg-emerald-950/30 dark:text-emerald-300 dark:border-emerald-700",
      inactive:
        "bg-transparent text-muted-foreground border-border hover:border-emerald-300 hover:text-emerald-700 dark:hover:border-emerald-700 dark:hover:text-emerald-300",
    },
    community_verified: {
      active:
        "bg-sky-50 text-sky-700 border-sky-300 dark:bg-sky-950/30 dark:text-sky-300 dark:border-sky-700",
      inactive:
        "bg-transparent text-muted-foreground border-border hover:border-sky-300 hover:text-sky-700 dark:hover:border-sky-700 dark:hover:text-sky-300",
    },
    community: {
      active:
        "bg-amber-50 text-amber-700 border-amber-300 dark:bg-amber-950/30 dark:text-amber-300 dark:border-amber-700",
      inactive:
        "bg-transparent text-muted-foreground border-border hover:border-amber-300 hover:text-amber-700 dark:hover:border-amber-700 dark:hover:text-amber-300",
    },
    custom: {
      active: "bg-muted text-foreground border-border",
      inactive: "bg-transparent text-muted-foreground border-border hover:text-foreground",
    },
  };

  const LABEL_KEYS: Record<TrustFilterOption, string> = {
    verified_official: "integrations.trust.verified_official",
    community_verified: "integrations.trust.community_verified",
    community: "integrations.trust.community",
    custom: "integrations.trust.custom",
  };

  function toggle(option: TrustFilterOption): void {
    const next = selected.includes(option)
      ? selected.filter((o) => o !== option)
      : [...selected, option];
    onchange(next);
  }
</script>

<div class="flex flex-wrap items-center gap-2" data-testid="catalogue-trust-filter">
  <span class="text-sm text-muted-foreground whitespace-nowrap">
    {$t("integrations.catalogue.filter_trust_label")}
  </span>

  {#each ALL_OPTIONS as option (option)}
    {@const active = selected.includes(option)}
    <button
      type="button"
      role="checkbox"
      aria-checked={active}
      class={cn(
        "inline-flex items-center rounded-full border px-2.5 py-0.5 text-xs font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40 focus-visible:ring-offset-2",
        active ? CHIP_CLASSES[option].active : CHIP_CLASSES[option].inactive,
      )}
      onclick={() => toggle(option)}
      data-testid={`trust-chip-${option}`}
    >
      {$t(LABEL_KEYS[option])}
    </button>
  {/each}
</div>
