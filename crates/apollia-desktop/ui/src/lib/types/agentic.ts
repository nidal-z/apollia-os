// ─── Tools, workers and the agentic layer ───

// ─── Système Agentique Amélioré ─────────────────────────────────

/** Résumé d'un outil pour l'affichage en liste. */
export interface ToolSummary {
  /** Nom unique de l'outil (ex: "bash_executor"). */
  name: string;
  /** Version semver de l'outil. */
  version: string;
  /** Description humaine de l'outil. */
  description: string;
  /** Type d'outil : "native", "mcp", "python". */
  kind: string;
}

/** Vue détaillée d'un outil pour l'introspection (miroir Rust ToolDescriptor). */
export interface ToolDescriptorView {
  /** Nom unique de l'outil (ex: "bash_executor"). */
  name: string;
  /** Version semver de l'outil. */
  version: string;
  /** Description humaine de l'outil. */
  description: string;
  /** Type d'outil : "native", "mcp", "python". */
  kind: string;
  /** JSON Schema d'entrée (`null` si non défini). */
  input_schema: Record<string, unknown> | null;
  /** JSON Schema de sortie (`null` si non défini). */
  output_schema: Record<string, unknown> | null;
  /** Permissions requises par l'outil. */
  permissions: string[];
}

/** Statistiques du cache de plans ORIA. */
export interface PlanCacheStats {
  /** Nombre total d'entrées en cache. */
  total_entries: number;
  /** Nombre total de cache hits depuis le démarrage. */
  cache_hits: number;
  /** Nombre total de cache misses depuis le démarrage. */
  cache_misses: number;
  /** Taux de hit en pourcentage (0-100). */
  hit_rate_pct: number;
  /** Horodatage RFC 3339 de l'entrée la plus ancienne (`null` si cache vide). */
  oldest_entry_at: string | null;
  /** Horodatage RFC 3339 de l'entrée la plus récente (`null` si cache vide). */
  newest_entry_at: string | null;
}

/** Message échangé entre deux agents via la mailbox. */
export interface AgentMessage {
  /** Identifiant unique du message. */
  id: string;
  /** Nom de l'agent expéditeur. */
  from_agent: string;
  /** Nom de l'agent destinataire. */
  to_agent: string;
  /** Contenu du message (JSON arbitraire). */
  payload: Record<string, unknown>;
  /** Horodatage d'envoi RFC 3339. */
  sent_at: string;
}
