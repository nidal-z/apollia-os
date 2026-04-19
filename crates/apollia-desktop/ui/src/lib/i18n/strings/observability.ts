/** Observability dashboard: timeline, LLM costs, audit trail, plan cache. */
export const OBSERVABILITY_KEYS = {
  root: "observability",
  title: "observability.title",
  planCache: {
    title: "observability.plan_cache.title",
    purgeButton: "observability.plan_cache.purge_button",
    emptyTitle: "observability.plan_cache.empty_title",
    emptySubtitle: "observability.plan_cache.empty_subtitle",
    totalEntries: "observability.plan_cache.total_entries",
    hitRate: "observability.plan_cache.hit_rate",
    cacheHits: "observability.plan_cache.cache_hits",
    cacheMisses: "observability.plan_cache.cache_misses",
    dialogTitle: "observability.plan_cache.dialog_title",
    dialogMessage: "observability.plan_cache.dialog_message",
    dialogConfirm: "observability.plan_cache.dialog_confirm",
    toastPurged: "observability.plan_cache.toast_purged",
  },
} as const;
