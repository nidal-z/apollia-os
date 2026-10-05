import { describe, test, expect } from "vitest";
import { get } from "svelte/store";
import type { DownloadProgress, GgufModelInfo } from "$lib/ipc/models";
import {
  createLlmDownload,
  downloadNotice,
  downloadPercent,
  isInFlight,
  type LlmDownloadDeps,
} from "./llmDownload";

// Vitest runs in `node`: these tests drive the object the step and the header
// of the modal read, with the IPC replaced by a recorder. No component is
// mounted, which is the point: the download is followed with none on screen.

const FILE = "Qwen3-8B-Q4_K_M.gguf";
const ON_DISK: GgufModelInfo = {
  path: `/models/${FILE}`,
  filename: FILE,
  size_bytes: 5_030_000_000,
  size_human: "5.0 GB",
  recommended: true,
};

function progress(status: DownloadProgress["status"], downloaded = 0): DownloadProgress {
  return {
    id: "dl-1",
    downloaded_bytes: downloaded,
    total_bytes: 5_000_000_000,
    speed_bps: 18_000_000,
    dest_path: ON_DISK.path,
    status,
  };
}

function harness(overrides: Partial<LlmDownloadDeps> = {}) {
  const calls: string[] = [];
  const deps: LlmDownloadDeps = {
    cancelDownload: async (id) => {
      calls.push(`cancel:${id}`);
    },
    scan: async () => {
      calls.push("scan");
      return [ON_DISK];
    },
    wire: async (path) => {
      calls.push(`wire:${path}`);
    },
    onProgress: async () => {
      calls.push("listen");
      return () => {};
    },
    ...overrides,
  };
  return { download: createLlmDownload(deps), calls };
}

const launch = async (): Promise<string> => "dl-1";

describe("llmDownload - a download launched", () => {
  test("is in flight from the click, before any byte arrives", async () => {
    // GIVEN nothing asked for yet
    const { download } = harness();
    expect(isInFlight(get(download))).toBe(false);

    // WHEN the download of a model is launched
    await download.start(launch, FILE, 5_000_000_000);

    // THEN it is in flight at once, which is what opens "Continue"
    const state = get(download);
    expect(state.phase).toBe("downloading");
    expect(isInFlight(state)).toBe(true);
    expect(state.filename).toBe(FILE);
    expect(state.sizeBytes).toBe(5_000_000_000);
  });

  test("listens once, however many downloads follow", async () => {
    // GIVEN a download that was cancelled
    const { download, calls } = harness();
    await download.start(launch, FILE, null);
    await download.handleProgress(progress("cancelled"));

    // WHEN another one is launched
    await download.start(launch, FILE, null);

    // THEN the progress events were subscribed to a single time
    expect(calls.filter((call) => call === "listen")).toHaveLength(1);
  });

  test("a second click while it runs starts nothing", async () => {
    // GIVEN a download in flight
    const { download } = harness();
    await download.start(launch, FILE, null);
    let launched = 0;

    // WHEN another model is asked for
    await download.start(
      async () => {
        launched += 1;
        return "dl-2";
      },
      "other.gguf",
      null,
    );

    // THEN the first one is still the one followed
    expect(launched).toBe(0);
    expect(get(download).filename).toBe(FILE);
  });

  test("a launch the backend refuses is a failure it names", async () => {
    // GIVEN a backend that refuses the download
    const { download } = harness();

    // WHEN the download is launched
    await download.start(
      async () => {
        throw new Error("start_variant_download refused");
      },
      FILE,
      null,
    );

    // THEN nothing is in flight, and the backend's sentence is kept
    const state = get(download);
    expect(isInFlight(state)).toBe(false);
    expect(state.phase).toBe("failed");
    expect(state.failure).toEqual({ kind: "start", detail: "start_variant_download refused" });
  });
});

describe("llmDownload - the file lands after its step was left", () => {
  test("it is wired as the engine with no component on screen", async () => {
    // GIVEN a download in flight, followed by this object alone
    const { download, calls } = harness();
    await download.start(launch, FILE, 5_000_000_000);
    await download.handleProgress(progress("in_progress", 1_900_000_000));
    expect(downloadPercent(get(download))).toBe(38);

    // WHEN the file lands
    await download.handleProgress(progress("completed", 5_000_000_000));

    // THEN the file found on disk is wired, and the state says which
    expect(calls).toContain("scan");
    expect(calls).toContain(`wire:${ON_DISK.path}`);
    const state = get(download);
    expect(state.phase).toBe("ready");
    expect(state.wiredPath).toBe(ON_DISK.path);
    expect(state.sizeBytes).toBe(ON_DISK.size_bytes);
    expect(isInFlight(state)).toBe(false);
  });

  test("it stays in flight while the file is being wired", async () => {
    // GIVEN a wiring that has not returned yet
    let release: () => void = () => {};
    const { download } = harness({
      wire: () =>
        new Promise<void>((resolve) => {
          release = resolve;
        }),
    });
    await download.start(launch, FILE, null);

    // WHEN the file lands
    const landing = download.handleProgress(progress("completed"));
    await Promise.resolve();
    await Promise.resolve();

    // THEN the step that needs the model still waits
    expect(get(download).phase).toBe("installing");
    expect(isInFlight(get(download))).toBe(true);
    release();
    await landing;
    expect(get(download).phase).toBe("ready");
  });

  test("an engine chosen by hand meanwhile is not replaced", async () => {
    // GIVEN a download in flight, and an engine wired by hand while it runs
    const { download, calls } = harness();
    await download.start(launch, FILE, null);
    download.keepCurrentEngine();

    // WHEN the file lands
    await download.handleProgress(progress("completed"));

    // THEN it is kept on disk and the choice made by hand stands
    expect(calls.some((call) => call.startsWith("wire:"))).toBe(false);
    expect(get(download).phase).toBe("ready");
    expect(get(download).wiredPath).toBeNull();
  });

  test("a file that cannot be wired is a failure, not a model announced ready", async () => {
    // GIVEN a backend that refuses the engine
    const { download } = harness({
      wire: async () => {
        throw new Error("engine refused the file");
      },
    });
    await download.start(launch, FILE, null);

    // WHEN the file lands
    await download.handleProgress(progress("completed"));

    // THEN the failure is kept with the backend's sentence
    const state = get(download);
    expect(state.phase).toBe("failed");
    expect(state.failure).toEqual({ kind: "install", detail: "engine refused the file" });
    expect(state.wiredPath).toBeNull();
  });

  test("a file the scan does not find is a failure too", async () => {
    // GIVEN a scan that does not list the downloaded file
    const { download } = harness({ scan: async () => [] });
    await download.start(launch, FILE, null);

    // WHEN the download reports its completion
    await download.handleProgress(progress("completed"));

    // THEN nothing is wired and the state says so
    expect(get(download).phase).toBe("failed");
    expect(get(download).failure?.kind).toBe("install");
  });
});

describe("llmDownload - a download that does not end well", () => {
  test("a failed download is no longer in flight", async () => {
    // GIVEN a download in flight
    const { download } = harness();
    await download.start(launch, FILE, null);

    // WHEN the backend reports that it failed
    await download.handleProgress(progress("failed"));

    // THEN "Continue" closes again, and the failure is shown
    const state = get(download);
    expect(isInFlight(state)).toBe(false);
    expect(state.failure).toEqual({ kind: "download", detail: null });
  });

  test("a cancelled download leaves nothing behind", async () => {
    // GIVEN a download in flight
    const { download, calls } = harness();
    await download.start(launch, FILE, null);

    // WHEN it is cancelled and the backend confirms
    await download.cancel();
    await download.handleProgress(progress("cancelled"));

    // THEN the state is the one of a step where nothing was asked
    expect(calls).toContain("cancel:dl-1");
    expect(get(download)).toMatchObject({ phase: "idle", filename: null, failure: null });
  });

  test("a cancel the backend refuses still frees the step", async () => {
    // GIVEN a download the backend no longer knows
    const { download } = harness({
      cancelDownload: async () => {
        throw new Error("no such download");
      },
    });
    await download.start(launch, FILE, null);

    // WHEN it is cancelled
    await download.cancel();

    // THEN the step is not left on a progress bar no event will ever end
    expect(get(download).phase).toBe("idle");
  });

  test("the events of another download are ignored", async () => {
    // GIVEN a download in flight
    const { download } = harness();
    await download.start(launch, FILE, null);

    // WHEN another download of the window completes
    await download.handleProgress({ ...progress("completed"), id: "someone-else" });

    // THEN this one is still being fetched
    expect(get(download).phase).toBe("downloading");
  });
});

describe("llmDownload - what the header of the modal says", () => {
  test("nothing while no download was asked for", () => {
    // GIVEN a fresh object
    const { download } = harness();

    // WHEN the header reads it
    // THEN it shows nothing
    expect(downloadNotice(get(download))).toBeNull();
  });

  test("the percentage while it runs, on whatever step is open", async () => {
    // GIVEN a download at 42 percent
    const { download } = harness();
    await download.start(launch, FILE, null);
    await download.handleProgress(progress("in_progress", 2_100_000_000));

    // WHEN the header reads it
    // THEN it shows the percentage
    expect(downloadNotice(get(download))).toEqual({
      key: "onboarding_modal.model_percent",
      values: { percent: 42 },
      tone: "progress",
    });
  });

  test("a sentence without a number while the size is unknown", async () => {
    // GIVEN a download that has not reported its size yet
    const { download } = harness();
    await download.start(launch, FILE, null);

    // WHEN the header reads it
    // THEN it still says that the model is on its way
    expect(downloadNotice(get(download))).toEqual({
      key: "onboarding_modal.model_downloading",
      tone: "progress",
    });
  });

  test("ready, then failed, each in its own tone", async () => {
    // GIVEN a download that lands, and another that fails
    const landed = harness();
    await landed.download.start(launch, FILE, null);
    await landed.download.handleProgress(progress("completed"));
    const failed = harness();
    await failed.download.start(launch, FILE, null);
    await failed.download.handleProgress(progress("failed"));

    // WHEN the header reads each
    // THEN the first says the model is ready and the second that it stopped
    expect(downloadNotice(get(landed.download))?.tone).toBe("ready");
    expect(downloadNotice(get(failed.download))?.key).toBe("onboarding_modal.model_failed");
  });
});
