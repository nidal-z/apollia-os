/**
 * Converts trigger source configuration into human-readable descriptions.
 *
 * Supports EN and FR locales. Falls back to the raw config string for
 * unrecognized patterns.
 */

type Locale = "en" | "fr";

const CRON_PATTERNS: Record<string, Record<Locale, string>> = {
  "* * * * *": { en: "Every minute", fr: "Toutes les minutes" },
  "0 * * * *": { en: "Every hour", fr: "Toutes les heures" },
  "0 0 * * *": { en: "Every day at midnight", fr: "Tous les jours à minuit" },
  "0 0 * * 0": { en: "Every Sunday at midnight", fr: "Chaque dimanche à minuit" },
  "0 0 1 * *": { en: "First day of every month", fr: "Premier jour de chaque mois" },
};

/**
 * Humanizes a cron expression into natural language.
 *
 * Recognizes common patterns and `* /N` step expressions.
 * Falls back to the raw expression for complex schedules.
 */
function humanizeCron(expr: string, locale: Locale): string {
  const trimmed = expr.trim();

  const exact = CRON_PATTERNS[trimmed];
  if (exact) {
    return exact[locale];
  }

  const parts = trimmed.split(/\s+/);
  if (parts.length < 5) {
    return trimmed;
  }

  const [minute, hour, dayOfMonth, month, dayOfWeek] = parts;

  // "*/N * * * *" → Every N minutes
  if (minute.startsWith("*/") && hour === "*" && dayOfMonth === "*" && month === "*" && dayOfWeek === "*") {
    const n = parseInt(minute.slice(2), 10);
    if (!Number.isNaN(n) && n > 0) {
      return locale === "fr" ? `Toutes les ${n} minutes` : `Every ${n} minutes`;
    }
  }

  // "0 */N * * *" → Every N hours
  if (minute === "0" && hour.startsWith("*/") && dayOfMonth === "*" && month === "*" && dayOfWeek === "*") {
    const n = parseInt(hour.slice(2), 10);
    if (!Number.isNaN(n) && n > 0) {
      return locale === "fr" ? `Toutes les ${n} heures` : `Every ${n} hours`;
    }
  }

  // "0 H * * *" → Daily at H:00
  if (minute === "0" && /^\d{1,2}$/.test(hour) && dayOfMonth === "*" && month === "*" && dayOfWeek === "*") {
    const h = hour.padStart(2, "0");
    return locale === "fr" ? `Tous les jours à ${h}:00` : `Daily at ${h}:00`;
  }

  // "M H * * *" → Daily at H:MM
  if (/^\d{1,2}$/.test(minute) && /^\d{1,2}$/.test(hour) && dayOfMonth === "*" && month === "*" && dayOfWeek === "*") {
    const h = hour.padStart(2, "0");
    const m = minute.padStart(2, "0");
    return locale === "fr" ? `Tous les jours à ${h}:${m}` : `Daily at ${h}:${m}`;
  }

  // "0 H * * 1-5" → Weekdays at H:00
  if (minute === "0" && /^\d{1,2}$/.test(hour) && dayOfMonth === "*" && month === "*" && dayOfWeek === "1-5") {
    const h = hour.padStart(2, "0");
    return locale === "fr" ? `Jours ouvrés à ${h}:00` : `Weekdays at ${h}:00`;
  }

  return trimmed;
}

/**
 * Parses an interval string (e.g. "30m", "2h", "90s", "1d") into seconds.
 *
 * Supports suffixes: `s` (seconds), `m` (minutes), `h` (hours), `d` (days).
 * Falls back to parsing as raw seconds if no suffix is found.
 */
function parseIntervalSeconds(config: string): number {
  const trimmed = config.trim().toLowerCase();
  const match = trimmed.match(/^(\d+)\s*([smhd])?$/);
  if (!match) {
    return NaN;
  }
  const value = parseInt(match[1], 10);
  const unit = match[2] ?? "s";
  switch (unit) {
    case "s": return value;
    case "m": return value * 60;
    case "h": return value * 3600;
    case "d": return value * 86400;
    default: return value;
  }
}

/**
 * Converts a trigger source into a human-readable description.
 *
 * @param sourceKind - The type of trigger source (cron, interval, file_watch, webhook, oneshot).
 * @param config - The raw configuration string (cron expression, interval duration, file path, etc.).
 * @param locale - The locale code ("en" or "fr"). Defaults to "en" for unsupported locales.
 * @returns A human-readable description of the trigger schedule.
 */
export function humanizeTrigger(sourceKind: string, config: string, locale: string): string {
  const loc: Locale = locale.startsWith("fr") ? "fr" : "en";

  switch (sourceKind) {
    case "cron":
      return humanizeCron(config, loc);
    case "interval": {
      const seconds = parseIntervalSeconds(config);
      if (Number.isNaN(seconds) || seconds <= 0) {
        return config;
      }
      if (seconds >= 86400) {
        const d = seconds / 86400;
        return loc === "fr"
          ? `Tous les ${d} jour${d > 1 ? "s" : ""}`
          : `Every ${d} day${d > 1 ? "s" : ""}`;
      }
      if (seconds >= 3600) {
        const h = seconds / 3600;
        return loc === "fr"
          ? `Toutes les ${h} heure${h > 1 ? "s" : ""}`
          : `Every ${h} hour${h > 1 ? "s" : ""}`;
      }
      if (seconds >= 60) {
        const m = seconds / 60;
        return loc === "fr"
          ? `Toutes les ${m} minute${m > 1 ? "s" : ""}`
          : `Every ${m} minute${m > 1 ? "s" : ""}`;
      }
      return loc === "fr"
        ? `Toutes les ${seconds} seconde${seconds > 1 ? "s" : ""}`
        : `Every ${seconds} second${seconds > 1 ? "s" : ""}`;
    }
    case "file_watch":
      return loc === "fr"
        ? `Quand un fichier change dans ${config}`
        : `When a file changes in ${config}`;
    case "webhook":
      return "Via webhook";
    case "oneshot":
      return loc === "fr" ? "Déclenchement unique" : "One-time trigger";
    default:
      return sourceKind;
  }
}
