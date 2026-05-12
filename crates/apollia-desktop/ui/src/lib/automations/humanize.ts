/**
 * Human-friendly labels for automation schedules.
 *
 * Operator vocabulary — never exposes cron digits, payload, or on-busy.
 * Returns a complete sentence usable inline in an `AutomationScheduleLabel`.
 * Falls back to "Custom schedule" / "Planification personnalisée" with the
 * raw config moved to a tooltip by the caller.
 */

export type ScheduleKind = "cron" | "interval" | "file_watch" | "webhook" | "oneshot";
export type Locale = "en" | "fr";

const CRON_EXACT: Record<string, Record<Locale, string>> = {
  "* * * * *": { en: "Every minute", fr: "Toutes les minutes" },
  "0 * * * *": { en: "Every hour on the hour", fr: "Toutes les heures pile" },
  "0 0 * * *": { en: "Every day at midnight", fr: "Tous les jours à minuit" },
  "0 12 * * *": { en: "Every day at noon", fr: "Tous les jours à midi" },
  "0 0 * * 0": { en: "Every Sunday at midnight", fr: "Chaque dimanche à minuit" },
  "0 0 * * 1": { en: "Every Monday at midnight", fr: "Chaque lundi à minuit" },
  "0 0 1 * *": { en: "First day of every month", fr: "Le 1er de chaque mois" },
};

const DOW_LABEL_EN = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const DOW_LABEL_FR = ["dimanche", "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi"];

function pad(n: string | number): string {
  return String(n).padStart(2, "0");
}

function humanizeCron(expr: string, loc: Locale): string | null {
  const trimmed = expr.trim();
  const exact = CRON_EXACT[trimmed];
  if (exact) return exact[loc];

  const parts = trimmed.split(/\s+/);
  if (parts.length < 5) return null;
  const [minute, hour, dayOfMonth, month, dayOfWeek] = parts;

  // */N * * * *  →  Every N minutes
  if (/^\*\/\d+$/.test(minute) && hour === "*" && dayOfMonth === "*" && month === "*" && dayOfWeek === "*") {
    const n = parseInt(minute.slice(2), 10);
    if (n > 0) {
      return loc === "fr" ? `Toutes les ${n} minutes` : `Every ${n} minutes`;
    }
  }

  // 0 */N * * *  →  Every N hours
  if (minute === "0" && /^\*\/\d+$/.test(hour) && dayOfMonth === "*" && month === "*" && dayOfWeek === "*") {
    const n = parseInt(hour.slice(2), 10);
    if (n > 0) {
      return loc === "fr" ? `Toutes les ${n} heures` : `Every ${n} hours`;
    }
  }

  // M H * * 1-5  →  weekdays at H:MM
  if (/^\d{1,2}$/.test(minute) && /^\d{1,2}$/.test(hour) && dayOfMonth === "*" && month === "*" && dayOfWeek === "1-5") {
    const hRaw = parseInt(hour, 10);
    const mRaw = parseInt(minute, 10);
    return loc === "fr"
      ? `Tous les matins à ${hRaw}h${mRaw === 0 ? "" : pad(mRaw)} en semaine`
      : `Every weekday at ${pad(hRaw)}:${pad(mRaw)}`;
  }

  // M H * * 0,6  →  weekends
  if (/^\d{1,2}$/.test(minute) && /^\d{1,2}$/.test(hour) && dayOfMonth === "*" && month === "*" && (dayOfWeek === "0,6" || dayOfWeek === "6,0")) {
    const hRaw = parseInt(hour, 10);
    const mRaw = parseInt(minute, 10);
    return loc === "fr"
      ? `Le week-end à ${hRaw}h${mRaw === 0 ? "" : pad(mRaw)}`
      : `On weekends at ${pad(hRaw)}:${pad(mRaw)}`;
  }

  // M H * * D  →  every <day> at H:MM
  if (/^\d{1,2}$/.test(minute) && /^\d{1,2}$/.test(hour) && dayOfMonth === "*" && month === "*" && /^[0-6]$/.test(dayOfWeek)) {
    const hRaw = parseInt(hour, 10);
    const mRaw = parseInt(minute, 10);
    const d = parseInt(dayOfWeek, 10);
    return loc === "fr"
      ? `Chaque ${DOW_LABEL_FR[d]} à ${hRaw}h${mRaw === 0 ? "" : pad(mRaw)}`
      : `Every ${DOW_LABEL_EN[d]} at ${pad(hRaw)}:${pad(mRaw)}`;
  }

  // M H * * *  →  daily at H:MM
  if (/^\d{1,2}$/.test(minute) && /^\d{1,2}$/.test(hour) && dayOfMonth === "*" && month === "*" && dayOfWeek === "*") {
    const hRaw = parseInt(hour, 10);
    const mRaw = parseInt(minute, 10);
    return loc === "fr"
      ? `Tous les jours à ${hRaw}h${mRaw === 0 ? "" : pad(mRaw)}`
      : `Every day at ${pad(hRaw)}:${pad(mRaw)}`;
  }

  // 0 H D * *  →  monthly on the Dth at H:00
  if (minute === "0" && /^\d{1,2}$/.test(hour) && /^\d{1,2}$/.test(dayOfMonth) && month === "*" && dayOfWeek === "*") {
    const hRaw = parseInt(hour, 10);
    const d = parseInt(dayOfMonth, 10);
    return loc === "fr"
      ? `Le ${d} de chaque mois à ${hRaw}h`
      : `Monthly on the ${d}${ordinalSuffix(d)} at ${pad(hRaw)}:00`;
  }

  return null;
}

function ordinalSuffix(n: number): string {
  const s = ["th", "st", "nd", "rd"];
  const v = n % 100;
  return s[(v - 20) % 10] ?? s[v] ?? s[0];
}

function humanizeInterval(config: string, loc: Locale): string | null {
  const trimmed = config.trim().toLowerCase();
  const match = trimmed.match(/^(\d+)\s*([smhd])?$/);
  if (!match) return null;
  const value = parseInt(match[1], 10);
  if (!Number.isFinite(value) || value <= 0) return null;
  const unit = match[2] ?? "s";
  const seconds =
    unit === "s" ? value :
    unit === "m" ? value * 60 :
    unit === "h" ? value * 3600 :
    value * 86400;

  if (seconds >= 86400 && seconds % 86400 === 0) {
    const d = seconds / 86400;
    return loc === "fr" ? `Tous les ${d} jour${d > 1 ? "s" : ""}` : `Every ${d} day${d > 1 ? "s" : ""}`;
  }
  if (seconds >= 3600 && seconds % 3600 === 0) {
    const h = seconds / 3600;
    return loc === "fr" ? `Toutes les ${h} heure${h > 1 ? "s" : ""}` : `Every ${h} hour${h > 1 ? "s" : ""}`;
  }
  if (seconds >= 60 && seconds % 60 === 0) {
    const m = seconds / 60;
    return loc === "fr" ? `Toutes les ${m} minutes` : `Every ${m} minutes`;
  }
  return loc === "fr"
    ? `Toutes les ${seconds} seconde${seconds > 1 ? "s" : ""}`
    : `Every ${seconds} second${seconds > 1 ? "s" : ""}`;
}

function extractWebhookSource(config: string): string | null {
  const trimmed = config.trim();
  if (!trimmed) return null;
  // Accept JSON {"path": "...", "source": "..."} or a plain string.
  try {
    const parsed = JSON.parse(trimmed) as Record<string, unknown>;
    const source = parsed.source ?? parsed.name ?? parsed.path;
    if (typeof source === "string" && source.length > 0) return source;
  } catch {
    // Not JSON — return first whitespace-free token.
  }
  const token = trimmed.split(/\s+/)[0];
  return token.length > 0 ? token : null;
}

/**
 * Turns a trigger definition into operator-friendly language.
 *
 * Returns `{ label, isCustom }` — when `isCustom` is `true`, the caller
 * should surface the raw expression through a tooltip.
 */
export function humanizeSchedule(
  kind: ScheduleKind | string,
  config: string,
  locale: string,
): { label: string; isCustom: boolean } {
  const loc: Locale = locale.startsWith("fr") ? "fr" : "en";

  switch (kind) {
    case "cron": {
      const label = humanizeCron(config, loc);
      if (label) return { label, isCustom: false };
      return {
        label: loc === "fr" ? "Planification personnalisée" : "Custom schedule",
        isCustom: true,
      };
    }
    case "interval": {
      const label = humanizeInterval(config, loc);
      if (label) return { label, isCustom: false };
      return {
        label: loc === "fr" ? "Planification personnalisée" : "Custom schedule",
        isCustom: true,
      };
    }
    case "file_watch": {
      const path = config.trim() || (loc === "fr" ? "un chemin" : "a path");
      return {
        label: loc === "fr" ? `Quand ${path} change` : `When ${path} changes`,
        isCustom: false,
      };
    }
    case "webhook": {
      const source = extractWebhookSource(config);
      return {
        label: source
          ? loc === "fr"
            ? `Quand Apollia reçoit un webhook depuis ${source}`
            : `When Apollia receives a webhook from ${source}`
          : loc === "fr"
            ? "Quand Apollia reçoit un webhook"
            : "When Apollia receives a webhook",
        isCustom: false,
      };
    }
    case "oneshot":
      return {
        label: loc === "fr" ? "Une seule fois" : "One time only",
        isCustom: false,
      };
    default:
      return {
        label: loc === "fr" ? "Planification personnalisée" : "Custom schedule",
        isCustom: true,
      };
  }
}

/**
 * Estimates the next run time from a cron or interval expression.
 *
 * Returns a Date or null. Only handles patterns also covered by
 * `humanizeSchedule()`; complex expressions yield null so the UI can
 * fall back to "Next run scheduled".
 */
export function estimateNextRun(
  kind: ScheduleKind | string,
  config: string,
  lastFired: Date | null,
  now: Date = new Date(),
): Date | null {
  if (kind === "interval") {
    const match = config.trim().toLowerCase().match(/^(\d+)\s*([smhd])?$/);
    if (!match) return null;
    const value = parseInt(match[1], 10);
    if (!Number.isFinite(value) || value <= 0) return null;
    const unit = match[2] ?? "s";
    const ms =
      unit === "s" ? value * 1000 :
      unit === "m" ? value * 60_000 :
      unit === "h" ? value * 3600_000 :
      value * 86_400_000;
    const base = lastFired ? lastFired.getTime() : now.getTime();
    const next = new Date(base + ms);
    return next.getTime() > now.getTime() ? next : new Date(now.getTime() + ms);
  }

  if (kind === "cron") {
    // Only handle the simple daily-at-H:M and weekday variants — good enough
    // for the card's "next run" label. Complex crons return null.
    const parts = config.trim().split(/\s+/);
    if (parts.length < 5) return null;
    const [minuteRaw, hourRaw, dayOfMonth, month, dayOfWeek] = parts;
    if (dayOfMonth !== "*" || month !== "*") return null;
    if (!/^\d{1,2}$/.test(minuteRaw) || !/^\d{1,2}$/.test(hourRaw)) return null;
    const minute = parseInt(minuteRaw, 10);
    const hour = parseInt(hourRaw, 10);
    const allowed = parseDowRange(dayOfWeek);
    if (!allowed) return null;

    const candidate = new Date(now);
    candidate.setSeconds(0, 0);
    candidate.setHours(hour, minute, 0, 0);
    for (let i = 0; i < 8; i++) {
      if (candidate.getTime() > now.getTime() && allowed.has(candidate.getDay())) {
        return candidate;
      }
      candidate.setDate(candidate.getDate() + 1);
    }
    return null;
  }

  return null;
}

function parseDowRange(spec: string): Set<number> | null {
  if (spec === "*") return new Set([0, 1, 2, 3, 4, 5, 6]);
  if (spec === "1-5") return new Set([1, 2, 3, 4, 5]);
  if (spec === "0,6" || spec === "6,0") return new Set([0, 6]);
  if (/^[0-6]$/.test(spec)) return new Set([parseInt(spec, 10)]);
  return null;
}

/**
 * Renders a natural-language countdown: "Next run in 2h34m",
 * "Next run tomorrow at 08:00". Past dates render as "Overdue".
 */
export function formatNextRun(next: Date | null, locale: string, now: Date = new Date()): string {
  const loc: Locale = locale.startsWith("fr") ? "fr" : "en";
  if (!next) {
    return loc === "fr" ? "Prochaine exécution planifiée" : "Next run scheduled";
  }
  const diffMs = next.getTime() - now.getTime();
  if (diffMs <= 0) {
    return loc === "fr" ? "En retard" : "Overdue";
  }
  const minutes = Math.floor(diffMs / 60_000);
  if (minutes < 1) {
    return loc === "fr" ? "Prochaine exécution dans <1 min" : "Next run in <1 min";
  }
  if (minutes < 60) {
    return loc === "fr" ? `Prochaine exécution dans ${minutes} min` : `Next run in ${minutes} min`;
  }
  const hours = Math.floor(minutes / 60);
  const remMin = minutes % 60;
  if (hours < 24) {
    return loc === "fr"
      ? `Prochaine exécution dans ${hours}h${pad(remMin)}`
      : `Next run in ${hours}h${pad(remMin)}`;
  }
  const days = Math.floor(hours / 24);
  return loc === "fr"
    ? `Prochaine exécution dans ${days} jour${days > 1 ? "s" : ""}`
    : `Next run in ${days} day${days > 1 ? "s" : ""}`;
}

/**
 * Returns the success rate percentage (0–100). Falls back to 100 when
 * no runs have been recorded yet.
 */
export function computeSuccessRate(fireCount: number, skipCount: number): number {
  const total = fireCount + skipCount;
  if (total === 0) return 100;
  return Math.round((fireCount / total) * 100);
}
