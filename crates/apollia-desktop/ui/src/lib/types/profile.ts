// ─── User profile ───

// ─── User Profile ───────────────────────────────────────────────────

/** A single profile entry returned by get_profile. */
export interface ProfileEntryView {
  key: string;
  value: string;
  /** Provenance tag: "onboarding", "user", or "agent:<name>". */
  written_by: string;
  created_at: string;
  updated_at: string;
  in_schema: boolean;
}

/** Aggregated user profile returned by get_profile. */
export interface UserProfileView {
  schema_entries: ProfileEntryView[];
  extras: ProfileEntryView[];
  entries: ProfileEntryView[];
  last_updated_at: string | null;
}

/** Request payload for set_profile_entry. */
export interface SetProfileEntryRequest {
  key: string;
  value: string;
}

/** Statistics for a single chat conversation session. */
export interface ConversationStatsView {
  message_count: number;
  summarized_count: number;
  context_usage_pct: number;
  user_memory_injected: boolean;
  cross_sessions_referenced: number;
}

/** Token-level breakdown of context window usage. */
export interface ContextWindowStats {
  messagesTokens: number;
  userMemoryTokens: number;
  summaryTokens: number;
  totalTokens: number;
  maxTokens: number;
}

/** Insight extracted from a chat conversation by LLM analysis. */
export interface InsightEntry {
  id: string;
  text: string;
  category: "preference" | "habit" | "context";
  confidence: number;
  source: string;
  /** Verbatim quote from the conversation that the insight was extracted from. */
  source_quote?: string;
  /** Short rationale provided by the extraction agent explaining why this insight was kept. */
  extraction_reasoning?: string;
}

/** Outcome of a memory-write proposal. */
export type MemoryWriteOutcome =
  | { kind: "accepted" }
  | { kind: "rejected"; reason: string };

/** An insight that was rejected, kept for audit in the "Rejected" tab. */
export interface RejectedInsightEntry extends InsightEntry {
  rejected_reason: string;
  rejected_at: string;
}

/** Événement de cache hit pour un plan d'exécution. */
export interface PlanCacheHitEvent {
  /** ID de la tâche qui a déclenché le cache hit. */
  task_id: string;
  /** Clé de cache SHA-256 qui a matché. */
  cache_key: string;
  /** ID du plan réutilisé depuis le cache. */
  plan_id: string;
  /** Horodatage RFC 3339 du cache hit. */
  timestamp: string;
}
