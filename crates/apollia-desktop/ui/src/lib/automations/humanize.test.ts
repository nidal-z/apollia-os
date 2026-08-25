import { describe, it, expect } from "vitest";
import { get } from "svelte/store";
import { locale, t, waitLocale } from "svelte-i18n";
import "$lib/i18n";
import {
  humanizeSchedule,
  estimateNextRun,
  formatNextRun,
  computeSuccessRate,
  stripSecondsField,
  type ScheduleLabel,
} from "./humanize";

/**
 * `humanizeSchedule` / `formatNextRun` return catalogue keys plus values;
 * these tests render them through the real `en.json` / `fr.json`, so both
 * the key wiring and the copy itself are pinned end to end.
 */
async function render(spec: ScheduleLabel, loc: "en" | "fr"): Promise<string> {
  locale.set(loc);
  await waitLocale();
  return get(t)(spec.key, { values: spec.values });
}

async function schedule(
  kind: Parameters<typeof humanizeSchedule>[0],
  config: string,
  loc: "en" | "fr",
): Promise<string> {
  return render(humanizeSchedule(kind, config, loc).label, loc);
}

describe("humanizeSchedule - cron", () => {
  // GIVEN standard cron expressions WHEN humanized THEN operator sentences.
  it("recognises every minute", async () => {
    expect(await schedule("cron", "* * * * *", "en")).toBe("Every minute");
    expect(await schedule("cron", "* * * * *", "fr")).toBe("Toutes les minutes");
  });

  it("recognises hourly on the hour", async () => {
    expect(await schedule("cron", "0 * * * *", "en")).toBe("Every hour on the hour");
    expect(await schedule("cron", "0 * * * *", "fr")).toBe("Toutes les heures pile");
  });

  it("recognises daily at midnight", async () => {
    expect(await schedule("cron", "0 0 * * *", "en")).toBe("Every day at midnight");
    expect(await schedule("cron", "0 0 * * *", "fr")).toBe("Tous les jours à minuit");
  });

  it("recognises daily at a specific time", async () => {
    expect(await schedule("cron", "30 14 * * *", "en")).toBe("Every day at 14:30");
    expect(await schedule("cron", "30 14 * * *", "fr")).toBe("Tous les jours à 14h30");
  });

  it("recognises weekday mornings", async () => {
    expect(await schedule("cron", "0 8 * * 1-5", "en")).toBe("Every weekday at 08:00");
    expect(await schedule("cron", "0 8 * * 1-5", "fr")).toBe(
      "Tous les matins à 8h en semaine",
    );
  });

  it("recognises weekends", async () => {
    expect(await schedule("cron", "30 9 * * 0,6", "en")).toBe("On weekends at 09:30");
    expect(await schedule("cron", "30 9 * * 0,6", "fr")).toBe("Le week-end à 9h30");
  });

  it("recognises a specific day of week", async () => {
    expect(await schedule("cron", "0 10 * * 3", "en")).toBe("Every Wednesday at 10:00");
    expect(await schedule("cron", "0 10 * * 3", "fr")).toBe("Chaque mercredi à 10h");
  });

  it("recognises every N minutes via step", async () => {
    expect(await schedule("cron", "*/15 * * * *", "en")).toBe("Every 15 minutes");
    expect(await schedule("cron", "*/15 * * * *", "fr")).toBe("Toutes les 15 minutes");
  });

  it("recognises every N hours via step", async () => {
    expect(await schedule("cron", "0 */3 * * *", "en")).toBe("Every 3 hours");
    expect(await schedule("cron", "0 */3 * * *", "fr")).toBe("Toutes les 3 heures");
  });

  it("recognises monthly on a given day", async () => {
    expect(await schedule("cron", "0 9 15 * *", "en")).toBe(
      "Monthly on the 15th at 09:00",
    );
    expect(await schedule("cron", "0 9 15 * *", "fr")).toBe("Le 15 de chaque mois à 9h");
  });

  it("falls back to custom-schedule for complex expressions", async () => {
    const res = humanizeSchedule("cron", "0 9,17 * * 1-5", "en");
    expect(res.isCustom).toBe(true);
    expect(await render(res.label, "en")).toBe("Custom schedule");
    expect(await schedule("cron", "0 9,17 * * 1-5", "fr")).toBe(
      "Planification personnalisée",
    );
  });

  it("falls back when the expression has fewer than 5 fields", () => {
    expect(humanizeSchedule("cron", "broken", "en").isCustom).toBe(true);
  });
});

describe("humanizeSchedule - cron persisted with a seconds field", () => {
  // GIVEN the 6-field form the runtime persists for scheduler presets
  // WHEN humanized THEN the label matches the 5-field original.
  it("recognises every 15 minutes behind a zero seconds field", async () => {
    expect(await schedule("cron", "0 */15 * * * *", "en")).toBe("Every 15 minutes");
    expect(await schedule("cron", "0 */15 * * * *", "fr")).toBe("Toutes les 15 minutes");
  });

  it("recognises a daily time behind a zero seconds field", async () => {
    expect(await schedule("cron", "0 30 14 * * *", "en")).toBe("Every day at 14:30");
    expect(await schedule("cron", "0 30 14 * * *", "fr")).toBe("Tous les jours à 14h30");
  });

  it("keeps a 6-field expression with non-zero seconds custom", () => {
    expect(humanizeSchedule("cron", "30 */15 * * * *", "en").isCustom).toBe(true);
  });

  it("estimates the next run behind a zero seconds field", () => {
    // GIVEN a daily 08:00 schedule in 6-field form and a 06:00 clock
    const now = new Date(2026, 7, 13, 6, 0, 0);
    // WHEN estimating THEN the next run is 08:00 the same day
    const next = estimateNextRun("cron", "0 0 8 * * *", null, now);
    expect(next?.getHours()).toBe(8);
    expect(next?.getDate()).toBe(13);
  });

  it("stripSecondsField leaves 5-field and non-zero-seconds expressions untouched", () => {
    expect(stripSecondsField("*/15 * * * *")).toBe("*/15 * * * *");
    expect(stripSecondsField("30 */15 * * * *")).toBe("30 */15 * * * *");
    expect(stripSecondsField("0 */15 * * * *")).toBe("*/15 * * * *");
  });
});

describe("humanizeSchedule - interval", () => {
  it("humanizes seconds", async () => {
    expect(await schedule("interval", "30s", "en")).toBe("Every 30 seconds");
    expect(await schedule("interval", "30s", "fr")).toBe("Toutes les 30 secondes");
  });

  it("humanizes half-hour intervals", async () => {
    expect(await schedule("interval", "30m", "en")).toBe("Every 30 minutes");
    expect(await schedule("interval", "30m", "fr")).toBe("Toutes les 30 minutes");
  });

  it("humanizes days", async () => {
    expect(await schedule("interval", "2d", "en")).toBe("Every 2 days");
    expect(await schedule("interval", "2d", "fr")).toBe("Tous les 2 jours");
  });

  it("falls back to custom when the interval is malformed", () => {
    expect(humanizeSchedule("interval", "nope", "en").isCustom).toBe(true);
  });
});

describe("humanizeSchedule - file_watch & webhook & oneshot", () => {
  it("describes a file watcher", async () => {
    expect(await schedule("file_watch", "~/Documents/specs", "en")).toBe(
      "When ~/Documents/specs changes",
    );
    expect(await schedule("file_watch", "~/Documents/specs", "fr")).toBe(
      "Quand ~/Documents/specs change",
    );
  });

  it("describes a webhook with a source", async () => {
    expect(await schedule("webhook", '{"source": "GitHub"}', "en")).toBe(
      "When Apollia receives a webhook from GitHub",
    );
    expect(await schedule("webhook", "stripe", "fr")).toBe(
      "Quand Apollia reçoit un webhook depuis stripe",
    );
  });

  it("describes a bare webhook when source is unknown", async () => {
    expect(await schedule("webhook", "", "en")).toBe(
      "When Apollia receives a webhook",
    );
  });

  it("describes a oneshot", async () => {
    expect(await schedule("oneshot", "", "en")).toBe("One time only");
    expect(await schedule("oneshot", "", "fr")).toBe("Une seule fois");
  });
});

describe("estimateNextRun", () => {
  it("adds the interval on top of lastFired", () => {
    const last = new Date("2026-01-01T10:00:00Z");
    const now = new Date("2026-01-01T10:05:00Z");
    const next = estimateNextRun("interval", "30m", last, now);
    expect(next?.toISOString()).toBe("2026-01-01T10:30:00.000Z");
  });

  it("rolls forward when the last run + interval is already past", () => {
    const last = new Date("2026-01-01T09:00:00Z");
    const now = new Date("2026-01-01T10:30:00Z");
    const next = estimateNextRun("interval", "30m", last, now);
    // Next run ends up 30min after `now`.
    expect(next?.getTime()).toBeGreaterThan(now.getTime());
  });

  it("finds the next daily fire for a cron", () => {
    const now = new Date();
    now.setHours(10, 0, 0, 0);
    const next = estimateNextRun("cron", "0 11 * * *", null, now);
    expect(next?.getHours()).toBe(11);
  });

  it("returns null for complex cron patterns", () => {
    const now = new Date("2026-01-01T10:00:00Z");
    expect(estimateNextRun("cron", "0 9,17 * * 1-5", null, now)).toBeNull();
  });
});

describe("formatNextRun", () => {
  const now = new Date("2026-01-01T10:00:00Z");

  it("formats minutes", async () => {
    const next = new Date(now.getTime() + 45 * 60_000);
    expect(await render(formatNextRun(next, "en", now), "en")).toBe(
      "Next run in 45 min",
    );
    expect(await render(formatNextRun(next, "fr", now), "fr")).toBe(
      "Prochaine exécution dans 45 min",
    );
  });

  it("formats hours and minutes", async () => {
    const next = new Date(now.getTime() + (2 * 60 + 34) * 60_000);
    expect(await render(formatNextRun(next, "fr", now), "fr")).toBe(
      "Prochaine exécution dans 2h34",
    );
  });

  it("formats days", async () => {
    const next = new Date(now.getTime() + 3 * 86_400_000);
    expect(await render(formatNextRun(next, "en", now), "en")).toBe(
      "Next run in 3 days",
    );
  });

  it("labels overdue runs", async () => {
    const next = new Date(now.getTime() - 60_000);
    expect(await render(formatNextRun(next, "en", now), "en")).toBe("Overdue");
    expect(await render(formatNextRun(next, "fr", now), "fr")).toBe("En retard");
  });

  it("falls back when next is unknown", async () => {
    expect(await render(formatNextRun(null, "en", now), "en")).toBe(
      "Next run scheduled",
    );
  });
});

describe("computeSuccessRate", () => {
  it("returns 100 when there are no runs", () => {
    expect(computeSuccessRate(0, 0)).toBe(100);
  });

  it("computes the rate as fires / (fires + skips)", () => {
    expect(computeSuccessRate(38, 2)).toBe(95);
    expect(computeSuccessRate(0, 5)).toBe(0);
    expect(computeSuccessRate(1, 1)).toBe(50);
  });
});
