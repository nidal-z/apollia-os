import { beforeEach, describe, expect, test, vi } from "vitest";
import { get } from "svelte/store";

type DropEvent = { payload: { type: string; paths?: string[] } };

const shell = vi.hoisted(() => ({
  invoke: vi.fn(async (_cmd: string): Promise<unknown> => []),
  listeners: new Map<string, () => void>(),
  unlistened: [] as string[],
  drop: null as ((event: DropEvent) => void) | null,
  webviewMissing: false,
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: shell.invoke }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, handler: () => void) => {
    shell.listeners.set(name, handler);
    return () => shell.unlistened.push(name);
  }),
}));
vi.mock("@tauri-apps/api/webview", () => ({
  getCurrentWebview: () => {
    if (shell.webviewMissing) throw new TypeError("no shell");
    return {
      onDragDropEvent: async (handler: (event: DropEvent) => void) => {
        shell.drop = handler;
        return () => shell.unlistened.push("drag-drop");
      },
    };
  },
}));

import { closePackagePreview, openPackagePreview, packagePreviewRequest } from "$lib/stores/packagePreview";
import {
  PACKAGE_OPENED_EVENT,
  isPackageFile,
  mountPackageOpening,
  openPackageAmong,
  packageAmong,
} from "./packageOpening";

/** Lets the promises of a mount, or of an event, settle. */
async function settle(): Promise<void> {
  for (let i = 0; i < 5; i += 1) await Promise.resolve();
}

function drop(paths: string[]): void {
  shell.drop?.({ payload: { type: "drop", paths } });
}

beforeEach(() => {
  closePackagePreview();
  shell.invoke.mockReset();
  shell.invoke.mockResolvedValue([]);
  shell.listeners.clear();
  shell.unlistened.length = 0;
  shell.drop = null;
  shell.webviewMissing = false;
});

describe("what counts as a package file", () => {
  test("the extension decides, whatever its case and the path style", () => {
    // GIVEN paths of both systems, a capital extension, and files that only
    // look like a package
    // WHEN each is asked whether it is a package file
    // THEN only a name ending in .apo is
    expect(isPackageFile("/Users/ada/Downloads/crm-1.0.0.apo")).toBe(true);
    expect(isPackageFile("C:\\Users\\ada\\crm.APO")).toBe(true);
    expect(isPackageFile("/Users/ada/notes.pdf")).toBe(false);
    expect(isPackageFile("/Users/ada/crm.apo.zip")).toBe(false);
    expect(isPackageFile("/Users/ada/apo")).toBe(false);
    expect(isPackageFile("/Users/ada/.apo")).toBe(false);
  });

  test("the first package among several files is the one kept", () => {
    // GIVEN files dropped together, two packages among them
    const paths = ["/tmp/notes.pdf", "/tmp/a.apo", "/tmp/b.apo"];

    // WHEN the package is looked for
    // THEN the first one is kept, and none is found among other files
    expect(packageAmong(paths)).toBe("/tmp/a.apo");
    expect(packageAmong(["/tmp/notes.pdf", "/tmp/agent.py"])).toBeNull();
  });
});

describe("a package dropped on the window", () => {
  test("opens the preview on its path", async () => {
    // GIVEN the window listening
    const stop = mountPackageOpening();
    await settle();

    // WHEN a package file is dropped on it
    drop(["/Users/ada/Downloads/crm-1.0.0.apo"]);

    // THEN the preview opens on that file, as from the file chooser
    expect(get(packagePreviewRequest)).toEqual({
      path: "/Users/ada/Downloads/crm-1.0.0.apo",
      origin: "files",
    });
    stop();
  });

  test("another type of file is ignored", async () => {
    // GIVEN the window listening
    const stop = mountPackageOpening();
    await settle();

    // WHEN a document and a script are dropped on it
    drop(["/Users/ada/notes.pdf", "/Users/ada/agent.py"]);

    // THEN no preview opens
    expect(get(packagePreviewRequest)).toBeNull();
    stop();
  });

  test("a file only dragged over the window opens nothing", async () => {
    // GIVEN the window listening
    const stop = mountPackageOpening();
    await settle();

    // WHEN a package enters and hovers the window without being dropped
    shell.drop?.({ payload: { type: "enter", paths: ["/tmp/crm.apo"] } });
    shell.drop?.({ payload: { type: "over" } });
    shell.drop?.({ payload: { type: "leave" } });

    // THEN no preview opens
    expect(get(packagePreviewRequest)).toBeNull();
    stop();
  });

  test("a preview already open is not swapped for another package", () => {
    // GIVEN the preview open on a package
    openPackagePreview("/tmp/a.apo");

    // WHEN another package arrives
    const opened = openPackageAmong(["/tmp/b.apo"]);

    // THEN the one in front of the user stays
    expect(opened).toBe(false);
    expect(get(packagePreviewRequest)?.path).toBe("/tmp/a.apo");
  });
});

describe("a package opened from the system", () => {
  test("the file that started the application opens its preview at mount", async () => {
    // GIVEN a package queued by the shell before the window mounted
    shell.invoke.mockResolvedValueOnce(["/home/ada/crm.apo"]);

    // WHEN the window starts listening
    const stop = mountPackageOpening();
    await settle();

    // THEN the queue was taken and the preview is open on that file
    expect(shell.invoke).toHaveBeenCalledWith("take_opened_package_files");
    expect(get(packagePreviewRequest)?.path).toBe("/home/ada/crm.apo");
    stop();
  });

  test("a file opened while the application runs opens its preview", async () => {
    // GIVEN the window listening, nothing queued at mount
    const stop = mountPackageOpening();
    await settle();
    expect(get(packagePreviewRequest)).toBeNull();

    // WHEN the shell says a package file was opened
    shell.invoke.mockResolvedValueOnce(["/home/ada/Downloads/veille.apo"]);
    shell.listeners.get(PACKAGE_OPENED_EVENT)?.();
    await settle();

    // THEN the preview opens on it
    expect(get(packagePreviewRequest)?.path).toBe("/home/ada/Downloads/veille.apo");
    stop();
  });

  test("stopping removes both listeners", async () => {
    // GIVEN the window listening
    const stop = mountPackageOpening();
    await settle();

    // WHEN it stops
    stop();

    // THEN the system event and the drop event are both released
    expect(shell.unlistened).toEqual([PACKAGE_OPENED_EVENT, "drag-drop"]);
  });

  test("a page without the desktop shell mounts without failing", async () => {
    // GIVEN a page served outside the shell: no webview, no command
    shell.webviewMissing = true;
    shell.invoke.mockRejectedValue(new Error("no shell"));

    // WHEN the page mounts
    const stop = mountPackageOpening();
    await settle();

    // THEN nothing throws and no preview opens
    expect(get(packagePreviewRequest)).toBeNull();
    stop();
  });
});
