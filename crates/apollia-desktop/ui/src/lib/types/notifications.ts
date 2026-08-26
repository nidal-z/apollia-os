// ─── Notification channels and history ───

/** Canal de notification configuré. */
export interface NotificationChannel {
  channel_id: string;
  /** Nom affiché libre. `null` ou absent = retombe sur `channel_id`. */
  label?: string | null;
  type: "desktop" | "webhook";
  enabled: boolean;
  events: string[];
  /** Intervalle minimal de throttling, en secondes (`0` = aucun). */
  min_interval_seconds?: number;
}

/** Définition complète d'un canal retournée par les opérations CRUD. */
export interface NotificationChannelView {
  id: string;
  /** Nom affiché libre. `null` = aucun label. */
  label: string | null;
  channel_type: "desktop" | "webhook";
  enabled: boolean;
  config: Record<string, unknown>;
  events: string[] | null;
  /** Intervalle minimal de throttling, en secondes (`0` = aucun). */
  min_interval_seconds: number;
  created_at: string;
  updated_at: string;
}

/** Corps de requête pour la création d'un canal de notification. */
export interface CreateChannelRequest {
  id: string;
  /** Nom affiché (libre, max 80 chars). Omettre si pas de label. */
  label?: string;
  channel_type: "desktop" | "webhook";
  enabled: boolean;
  config: Record<string, unknown>;
  events?: string[];
  /** Intervalle minimal de throttling, en secondes. Omettre = `0` (aucun). */
  min_interval_seconds?: number;
}

/**
 * Corps de requête pour la mise à jour d'un canal de notification.
 *
 * Sémantique du champ `label` :
 * - clé absente → conserver le label existant ;
 * - `label: null` → effacer le label ;
 * - `label: "texte"` → remplacer.
 */
export interface UpdateChannelRequest {
  label?: string | null;
  channel_type?: "desktop" | "webhook";
  enabled?: boolean;
  config?: Record<string, unknown>;
  events?: string[];
  /** Nouvel intervalle de throttling. Omettre = conserver l'existant. */
  min_interval_seconds?: number;
}

/** Résultat du test d'un canal de notification. */
export interface ChannelTestResult {
  channel_id: string;
  status: "ok" | "error" | "disabled";
  error: string | null;
  latency_ms: number | null;
}

/** Entrée de l'historique des notifications. */
export interface NotificationLogEntry {
  id: string;
  event_name: string;
  task_id: string | null;
  sent_at: string;
  channels: Record<string, string>;
  error: string | null;
}
