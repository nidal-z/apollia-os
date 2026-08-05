import { describe, it, expect } from "vitest";
import type { RedactedTriggerDefinition } from "$lib/ipc/triggers";
import {
  buildSource,
  emptyAutomationModel,
  isModelValid,
  modelFromDefinition,
  splitFireAt,
  toUpdateRequest,
  validateModel,
} from "./automationDefinition";

/**
 * Shape the host hands over: `source_config` already redacted, presence of a
 * secret reported by `has_secret`.
 */
function definition(
  overrides: Partial<RedactedTriggerDefinition>,
): RedactedTriggerDefinition {
  return {
    id: "daily-report",
    agent: "report-agent",
    enabled: true,
    on_busy: "queue",
    source_type: "cron",
    source_config: { schedule: "0 8 * * MON" },
    has_secret: false,
    input_template: null,
    created_at: "2026-03-20T10:00:00Z",
    updated_at: "2026-03-20T10:00:00Z",
    ...overrides,
  };
}

describe("modelFromDefinition", () => {
  it("maps a cron definition onto the form fields", () => {
    // GIVEN a stored cron trigger
    const stored = definition({ source_config: { schedule: "30 6 * * *" } });

    // WHEN projected onto the form model
    const model = modelFromDefinition(stored);

    // THEN the schedule and the target are prefilled
    expect(model.id).toBe("daily-report");
    expect(model.agent).toBe("report-agent");
    expect(model.sourceType).toBe("cron");
    expect(model.cronSchedule).toBe("30 6 * * *");
  });

  it("maps file-watch events onto the three toggles", () => {
    // GIVEN a stored file-watch trigger without the delete event
    const stored = definition({
      source_type: "file_watch",
      source_config: { path: "/data/inbox", events: ["create", "modify"], recursive: true },
    });

    // WHEN projected onto the form model
    const model = modelFromDefinition(stored);

    // THEN only the stored events are on
    expect(model.filePath).toBe("/data/inbox");
    expect(model.fileCreate).toBe(true);
    expect(model.fileModify).toBe(true);
    expect(model.fileDelete).toBe(false);
    expect(model.fileRecursive).toBe(true);
  });

  it("records the secret as stored without ever holding its value", () => {
    // GIVEN a redacted webhook definition: marker set, no secret in the config
    const stored = definition({
      source_type: "webhook",
      source_config: {},
      has_secret: true,
    });

    // WHEN projected onto the form model
    const model = modelFromDefinition(stored);

    // THEN the bound input stays empty and nothing but the marker is kept
    expect(model.webhookSecret).toBe("");
    expect(model.hasStoredWebhookSecret).toBe(true);
    expect(JSON.stringify(model)).not.toContain("secret\":\"s");
  });

  it("reports no stored secret when the marker is off", () => {
    // GIVEN a webhook definition the host reported as secret-less
    const stored = definition({
      source_type: "webhook",
      source_config: {},
      has_secret: false,
    });

    // WHEN projected onto the form model
    const model = modelFromDefinition(stored);

    // THEN the form knows it must ask for one
    expect(model.hasStoredWebhookSecret).toBe(false);
  });
});

describe("buildSource", () => {
  it("sends an empty secret when the operator does not rotate it", () => {
    // GIVEN a loaded webhook trigger the operator did not touch the secret of
    const model = modelFromDefinition(
      definition({ source_type: "webhook", source_config: {}, has_secret: true }),
    );

    // WHEN the source payload is built
    const source = buildSource(model);

    // THEN the empty field asks the host to keep the stored secret
    expect(source).toEqual({ type: "webhook", secret: "" });
  });

  it("uses the typed secret when the operator rotates it", () => {
    // GIVEN a loaded webhook trigger with a freshly typed secret
    const model = modelFromDefinition(
      definition({ source_type: "webhook", source_config: {}, has_secret: true }),
    );
    model.webhookSecret = "n3w";

    // WHEN the source payload is built
    const source = buildSource(model);

    // THEN the new secret replaces the stored one
    expect(source).toEqual({ type: "webhook", secret: "n3w" });
  });

  it("preserves config keys the form does not expose", () => {
    // GIVEN a file watch carrying an exclusion list the form cannot edit
    const model = modelFromDefinition(
      definition({
        source_type: "file_watch",
        source_config: {
          path: "/data/inbox",
          events: ["create"],
          recursive: false,
          exclude_patterns: [".git"],
        },
      }),
    );
    model.filePath = "/data/outbox";

    // WHEN the source payload is built
    const source = buildSource(model) as Record<string, unknown>;

    // THEN the edited field changes and the unknown key survives
    expect(source.path).toBe("/data/outbox");
    expect(source.exclude_patterns).toEqual([".git"]);
  });

  it("drops the previous config when the source type changes", () => {
    // GIVEN a cron trigger switched to an interval
    const model = modelFromDefinition(definition({}));
    model.sourceType = "interval";
    model.intervalEvery = "30m";

    // WHEN the source payload is built
    const source = buildSource(model) as Record<string, unknown>;

    // THEN nothing from the cron config leaks into it
    expect(source).toEqual({ type: "interval", every: "30m" });
  });
});

describe("one-shot timestamps", () => {
  it("round-trips a zoned timestamp through the pickers", () => {
    // GIVEN a stored one-shot with a zoned timestamp
    const stored = definition({
      source_type: "oneshot",
      source_config: { fire_at: "2026-08-01T06:30:00Z" },
    });

    // WHEN loaded and sent back untouched
    const model = modelFromDefinition(stored);
    const source = buildSource(model) as Record<string, unknown>;

    // THEN the same instant comes back, still zoned so the API accepts it
    expect(new Date(String(source.fire_at)).toISOString()).toBe("2026-08-01T06:30:00.000Z");
  });

  it("leaves the timestamp empty when a half is missing", () => {
    // GIVEN a one-shot with a date but no time
    const model = emptyAutomationModel();
    model.sourceType = "oneshot";
    model.oneshotDate = "2026-08-01";

    // WHEN the source payload is built
    const source = buildSource(model) as Record<string, unknown>;

    // THEN no timestamp is produced and validation can catch it
    expect(source.fire_at).toBe("");
    expect(validateModel(model, "edit").source).toBe("triggers.field_fire_at_required");
  });
});

describe("splitFireAt", () => {
  it("splits a naive local timestamp", () => {
    // GIVEN the shape the create path writes
    const value = "2026-08-01T08:30";

    // WHEN split for the pickers
    const parts = splitFireAt(value);

    // THEN both halves come back untouched
    expect(parts).toEqual({ date: "2026-08-01", time: "08:30" });
  });

  it("returns empty halves for an empty value", () => {
    // GIVEN no stored timestamp
    const value = "";

    // WHEN split for the pickers
    const parts = splitFireAt(value);

    // THEN nothing is prefilled
    expect(parts).toEqual({ date: "", time: "" });
  });
});

describe("validateModel", () => {
  it("accepts a legacy identifier in edit mode", () => {
    // GIVEN a stored trigger whose id predates the current grammar
    const model = modelFromDefinition(definition({ id: "Daily_Report" }));

    // WHEN validated on both paths
    const asCreate = validateModel(model, "create");
    const asEdit = validateModel(model, "edit");

    // THEN only the create path rejects it, the edit path stays usable
    expect(asCreate.id).toBe("triggers.field_id_invalid");
    expect(asEdit.id).toBeNull();
    expect(isModelValid(asEdit)).toBe(true);
  });

  it("requires a secret on a new webhook only", () => {
    // GIVEN a blank webhook on the create path
    const fresh = emptyAutomationModel();
    fresh.id = "hook";
    fresh.agent = "report-agent";
    fresh.sourceType = "webhook";

    // AND a stored webhook whose secret is untouched
    const stored = modelFromDefinition(
      definition({ source_type: "webhook", source_config: {}, has_secret: true }),
    );

    // WHEN both are validated
    const freshErrors = validateModel(fresh, "create");
    const storedErrors = validateModel(stored, "edit");

    // THEN the empty field only blocks the creation
    expect(freshErrors.source).toBe("triggers.field_secret_required");
    expect(storedErrors.source).toBeNull();
  });

  it("still demands a secret when the edited webhook has none stored", () => {
    // GIVEN a cron trigger the operator is switching to a webhook
    const model = modelFromDefinition(definition({}));
    model.sourceType = "webhook";

    // WHEN validated on the edit path
    const errors = validateModel(model, "edit");

    // THEN the absent presence marker keeps the field mandatory
    expect(errors.source).toBe("triggers.field_secret_required");
    expect(isModelValid(errors)).toBe(false);
  });

  it("reports a missing schedule", () => {
    // GIVEN a cron trigger without an expression
    const model = modelFromDefinition(definition({ source_config: {} }));

    // WHEN validated
    const errors = validateModel(model, "edit");

    // THEN the source error names the schedule
    expect(errors.source).toBe("triggers.field_schedule_required");
    expect(isModelValid(errors)).toBe(false);
  });
});

describe("toUpdateRequest", () => {
  it("omits an empty input template and keeps the target", () => {
    // GIVEN a stored trigger with no template
    const model = modelFromDefinition(definition({ input_template: null }));

    // WHEN the update payload is built
    const request = toUpdateRequest(model);

    // THEN the template key is absent and the rest is carried over
    expect(request).toEqual({
      agent: "report-agent",
      enabled: true,
      on_busy: "queue",
      source: { type: "cron", schedule: "0 8 * * MON" },
    });
  });

  it("carries the edited schedule and the busy policy", () => {
    // GIVEN a stored trigger whose hour and policy changed
    const model = modelFromDefinition(definition({}));
    model.cronSchedule = "0 9 * * MON";
    model.onBusy = "drop";
    model.inputTemplate = "  weekly report  ";

    // WHEN the update payload is built
    const request = toUpdateRequest(model);

    // THEN the payload reflects the edits, template trimmed
    expect(request?.source).toEqual({ type: "cron", schedule: "0 9 * * MON" });
    expect(request?.on_busy).toBe("drop");
    expect(request?.input_template).toBe("weekly report");
  });
});
