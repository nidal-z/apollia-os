import { describe, expect, test } from "vitest";
import {
  NOTICE_SEEN_KEY,
  noticeModel,
  readSeen,
  recordSeen,
  shouldShow,
  type MigrationReport,
} from "./notice";

/** A report of two projects, one document of which no longer exists. */
function reportWithAMissingDocument(): MigrationReport {
  return {
    formatVersion: 1,
    status: "complete",
    ranAt: "2026-10-04T08:00:00Z",
    projects: [
      {
        id: "p-1",
        name: "Dossier Martin",
        documentSources: ["src-1", "src-2"],
        instructionsSource: "src-3",
        agentsChecked: ["apollia"],
        folderConnection: "conn-1",
      },
      {
        id: "p-2",
        name: "Veille",
        documentSources: ["src-4"],
        instructionsSource: null,
        agentsChecked: ["apollia"],
        folderConnection: null,
      },
    ],
    conversationsDetached: 3,
    issues: [
      {
        projectId: "p-1",
        projectName: "Dossier Martin",
        item: "document",
        subject: "/Users/claire/Contrat-cadre 2025.pdf",
        reason: "missing",
        detail: null,
        blocking: false,
      },
    ],
  };
}

/** An in-memory stand-in for the local storage of the webview. */
function memoryStorage(): Pick<Storage, "getItem" | "setItem"> {
  const values = new Map<string, string>();
  return {
    getItem: (key) => values.get(key) ?? null,
    setItem: (key, value) => void values.set(key, value),
  };
}

describe("the notice of the migration of the projects", () => {
  test("says what was carried over and what was not", () => {
    // GIVEN a complete run over two projects, one document gone
    const report = reportWithAMissingDocument();

    // WHEN the notice is shaped
    const model = noticeModel(report);

    // THEN it counts the projects, documents, instructions and folders, and
    // lists the missing document with its project
    expect(model.kind).toBe("complete");
    expect(model).toMatchObject({ projects: 2, documents: 3, instructions: 1, folders: 1 });
    expect(model.notCarried).toHaveLength(1);
    expect(model.notCarried[0]).toMatchObject({
      projectName: "Dossier Martin",
      item: "document",
      reason: "missing",
    });
  });

  test("tells a partial run from a run that could read nothing", () => {
    // GIVEN a partial run that read the projects, and one that could not
    const partial = { ...reportWithAMissingDocument(), status: "partial" as const };
    const failed: MigrationReport = {
      ...partial,
      projects: [],
      issues: [
        { item: "store", subject: "projects.db", reason: "failed", detail: "locked", blocking: true },
      ],
    };

    // WHEN their notices are shaped
    const partialModel = noticeModel(partial);
    const failedModel = noticeModel(failed);

    // THEN the first lists what is left, the second says nothing was moved
    expect(partialModel.kind).toBe("partial");
    expect(partialModel.notCarried).toHaveLength(1);
    expect(failedModel.kind).toBe("failed");
    expect(failedModel.notCarried).toHaveLength(0);
  });

  test("is shown once per outcome, then never again", () => {
    // GIVEN no notice shown yet, and a partial run followed by a complete one
    const storage = memoryStorage();
    const partial = { ...reportWithAMissingDocument(), status: "partial" as const };
    const complete = reportWithAMissingDocument();

    // WHEN the partial notice is shown, then the complete one
    const firstPartial = shouldShow(partial, readSeen(storage));
    recordSeen(storage, partial);
    const againPartial = shouldShow(partial, readSeen(storage));
    const firstComplete = shouldShow(complete, readSeen(storage));
    recordSeen(storage, complete);
    const againComplete = shouldShow(complete, readSeen(storage));

    // THEN each shows the first time only
    expect([firstPartial, againPartial, firstComplete, againComplete]).toEqual([
      true,
      false,
      true,
      false,
    ]);
    expect(JSON.parse(storage.getItem(NOTICE_SEEN_KEY) ?? "[]")).toEqual(["partial", "complete"]);
  });

  test("shows nothing when no migration has run or the storage is unreadable", () => {
    // GIVEN no report, and a storage holding something that is not a list
    const storage = memoryStorage();
    storage.setItem(NOTICE_SEEN_KEY, "{not json");

    // WHEN asked whether to show
    const withoutReport = shouldShow(null, readSeen(storage));
    const withReport = shouldShow(reportWithAMissingDocument(), readSeen(storage));

    // THEN no report shows nothing, and an unreadable mark counts as unseen
    expect(withoutReport).toBe(false);
    expect(withReport).toBe(true);
  });
});
