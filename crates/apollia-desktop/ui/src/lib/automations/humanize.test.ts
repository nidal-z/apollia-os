import { describe, it, expect } from "vitest";
import {
  humanizeSchedule,
  estimateNextRun,
  formatNextRun,
  computeSuccessRate,
} from "./humanize";

describe("humanizeSchedule — cron", () => {
  // GIVEN standard cron expressions WHEN humanized THEN operator sentences.
  it("recognises every minute", () => {
    expect(humanizeSchedule("cron", "* * * * *", "en").label).toBe("Every minute");
    expect(humanizeSchedule("cron", "* * * * *", "fr").label).toBe("Toutes les minutes");
  });

  it("recognises hourly on the hour", () => {
    expect(humanizeSchedule("cron", "0 * * * *", "en").label).toBe("Every hour on the hour");
    expect(humanizeSchedule("cron", "0 * * * *", "fr").label).toBe("Toutes les heures pile");
  });

  it("recognises daily at midnight", () => {
    expect(humanizeSchedule("cron", "0 0 * * *", "en").label).toBe("Every day at midnight");
    expect(humanizeSchedule("cron", "0 0 * * *", "fr").label).toBe("Tous les jours à minuit");
  });

  it("recognises daily at a specific time", () => {
    expect(humanizeSchedule("cron", "30 14 * * *", "en").label).toBe("Every day at 14:30");
    expect(humanizeSchedule("cron", "30 14 * * *", "fr").label).toBe("Tous les jours à 14h30");
  });

  it("recognises weekday mornings", () => {
    expect(humanizeSchedule("cron", "0 8 * * 1-5", "en").label).toBe("Every weekday at 08:00");
    expect(humanizeSchedule("cron", "0 8 * * 1-5", "fr").label).toBe("Tous les matins à 8h en semaine");
  });

  it("recognises weekends", () => {
    expect(humanizeSchedule("cron", "30 9 * * 0,6", "en").label).toBe("On weekends at 09:30");
    expect(humanizeSchedule("cron", "30 9 * * 0,6", "fr").label).toBe("Le week-end à 9h30");
  });

  it("recognises a specific day of week", () => {
    expect(humanizeSchedule("cron", "0 10 * * 3", "en").label).toBe("Every Wednesday at 10:00");
    expect(humanizeSchedule("cron", "0 10 * * 3", "fr").label).toBe("Chaque mercredi à 10h");
  });

  it("recognises every N minutes via step", () => {
    expect(humanizeSchedule("cron", "*/15 * * * *", "en").label).toBe("Every 15 minutes");
    expect(humanizeSchedule("cron", "*/15 * * * *", "fr").label).toBe("Toutes les 15 minutes");
  });

  it("recognises every N hours via step", () => {
    expect(humanizeSchedule("cron", "0 */3 * * *", "en").label).toBe("Every 3 hours");
    expect(humanizeSchedule("cron", "0 */3 * * *", "fr").label).toBe("Toutes les 3 heures");
  });

  it("recognises monthly on a given day", () => {
    expect(humanizeSchedule("cron", "0 9 15 * *", "en").label).toBe("Monthly on the 15th at 09:00");
    expect(humanizeSchedule("cron", "0 9 15 * *", "fr").label).toBe("Le 15 de chaque mois à 9h");
  });

  it("falls back to custom-schedule for complex expressions", () => {
    const res = humanizeSchedule("cron", "0 9,17 * * 1-5", "en");
    expect(res.isCustom).toBe(true);
    expect(res.label).toBe("Custom schedule");
    expect(humanizeSchedule("cron", "0 9,17 * * 1-5", "fr").label).toBe("Planification personnalisée");
  });

  it("falls back when the expression has fewer than 5 fields", () => {
    expect(humanizeSchedule("cron", "broken", "en").isCustom).toBe(true);
  });
});

describe("humanizeSchedule — interval", () => {
  it("humanizes seconds", () => {
    expect(humanizeSchedule("interval", "30s", "en").label).toBe("Every 30 seconds");
    expect(humanizeSchedule("interval", "30s", "fr").label).toBe("Toutes les 30 secondes");
  });

  it("humanizes half-hour intervals", () => {
    expect(humanizeSchedule("interval", "30m", "en").label).toBe("Every 30 minutes");
    expect(humanizeSchedule("interval", "30m", "fr").label).toBe("Toutes les 30 minutes");
  });

  it("humanizes days", () => {
    expect(humanizeSchedule("interval", "2d", "en").label).toBe("Every 2 days");
    expect(humanizeSchedule("interval", "2d", "fr").label).toBe("Tous les 2 jours");
  });

  it("falls back to custom when the interval is malformed", () => {
    expect(humanizeSchedule("interval", "nope", "en").isCustom).toBe(true);
  });
});

describe("humanizeSchedule — file_watch & webhook & oneshot", () => {
  it("describes a file watcher", () => {
    expect(humanizeSchedule("file_watch", "~/Documents/specs", "en").label).toBe(
      "When a file changes in ~/Documents/specs",
    );
    expect(humanizeSchedule("file_watch", "~/Documents/specs", "fr").label).toBe(
      "Quand un fichier change dans ~/Documents/specs",
    );
  });

  it("describes a webhook with a source", () => {
    expect(humanizeSchedule("webhook", '{"source": "GitHub"}', "en").label).toBe(
      "When Apollia receives a webhook from GitHub",
    );
    expect(humanizeSchedule("webhook", "stripe", "fr").label).toBe(
      "Quand Apollia reçoit un webhook depuis stripe",
    );
  });

  it("describes a bare webhook when source is unknown", () => {
    expect(humanizeSchedule("webhook", "", "en").label).toBe("When Apollia receives a webhook");
  });

  it("describes a oneshot", () => {
    expect(humanizeSchedule("oneshot", "", "en").label).toBe("One time only");
    expect(humanizeSchedule("oneshot", "", "fr").label).toBe("Une seule fois");
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

  it("formats minutes", () => {
    const next = new Date(now.getTime() + 45 * 60_000);
    expect(formatNextRun(next, "en", now)).toBe("Next run in 45 min");
    expect(formatNextRun(next, "fr", now)).toBe("Prochaine exécution dans 45 min");
  });

  it("formats hours and minutes", () => {
    const next = new Date(now.getTime() + (2 * 60 + 34) * 60_000);
    expect(formatNextRun(next, "fr", now)).toBe("Prochaine exécution dans 2h34");
  });

  it("formats days", () => {
    const next = new Date(now.getTime() + 3 * 86_400_000);
    expect(formatNextRun(next, "en", now)).toBe("Next run in 3 days");
  });

  it("labels overdue runs", () => {
    const next = new Date(now.getTime() - 60_000);
    expect(formatNextRun(next, "en", now)).toBe("Overdue");
    expect(formatNextRun(next, "fr", now)).toBe("En retard");
  });

  it("falls back when next is unknown", () => {
    expect(formatNextRun(null, "en", now)).toBe("Next run scheduled");
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
