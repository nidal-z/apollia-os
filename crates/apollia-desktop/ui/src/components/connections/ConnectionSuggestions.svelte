<script lang="ts" module>
  import type { AgentListItem, RegistryServerView } from "$lib/types";

  const TRUST_SCORE: Record<string, number> = {
    verified_official: 4,
    community_verified: 3,
    community: 2,
    custom: 1,
  };

  /**
   * Rank a registry entry against the installed agent set.
   *
   * Score = trust_weight + category_match_boost. Installed entries are dropped
   * by the caller so they never surface here.
   */
  export function scoreSuggestion(
    server: RegistryServerView,
    installedCategories: Set<string>,
  ): number {
    const base = TRUST_SCORE[server.trust_level] ?? 0;
    const category = server.enrichment?.category ?? server.category;
    const boost = category && installedCategories.has(category) ? 3 : 0;
    return base + boost;
  }

  export function rankSuggestions(
    registry: RegistryServerView[],
    agents: AgentListItem[],
    limit = 6,
  ): RegistryServerView[] {
    const categories = new Set<string>();
    for (const agent of agents) {
      for (const tag of agent.tags ?? []) categories.add(tag);
    }
    const candidates = registry
      .filter((s) => !s.is_installed)
      .filter((s) => s.trust_level === "verified_official" || s.trust_level === "community_verified")
      .map((s) => ({ s, score: scoreSuggestion(s, categories) }))
      .sort((a, b) => b.score - a.score)
      .slice(0, limit)
      .map((entry) => entry.s);
    return candidates;
  }
</script>
