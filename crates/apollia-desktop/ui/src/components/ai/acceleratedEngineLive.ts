/**
 * The accelerated engine of the AI screen wired to the application: one
 * object for the life of the window, so an installation started on the
 * screen is still followed when the screen is shown again.
 */
import { get } from "svelte/store";
import { t } from "svelte-i18n";
import { open as openFolderPicker } from "@tauri-apps/plugin-dialog";
import { checkEngineUpdates, listEngines } from "$lib/ipc/system";
import {
  dismissEngineInstallation,
  getEngineInstallation,
  importEngineFolder,
  startEngineInstallation,
} from "$lib/ipc/engineInstallation";
import { listLoadedModels, unloadLoadedModel } from "$lib/ipc/llm";
import { loadModel } from "$lib/ipc/llmLoad";
import { reportError } from "$lib/errors/reportError";
import { createAcceleratedEngine } from "./acceleratedEngine";

/** The folder that holds the archive, the list and its signature, or `null`. */
async function pickFolder(): Promise<string | null> {
  const selected = await openFolderPicker({
    directory: true,
    multiple: false,
    title: get(t)("ai.engine.folder_picker_title"),
  });
  return typeof selected === "string" ? selected : null;
}

export const acceleratedEngine = createAcceleratedEngine({
  listEngines,
  checkUpdates: checkEngineUpdates,
  installation: getEngineInstallation,
  start: startEngineInstallation,
  importFolder: importEngineFolder,
  dismiss: dismissEngineInstallation,
  pickFolder,
  loaded: listLoadedModels,
  unload: unloadLoadedModel,
  load: loadModel,
  wait: (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
  now: () => Date.now(),
  onError: (err) => reportError(err, { surface: "toast" }),
});
