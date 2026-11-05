// The gestures of the Engines page wired to the application: one object for
// the life of the window, so an installation started on the page is still
// followed when the page is shown again.

import { get } from "svelte/store";
import { t } from "svelte-i18n";
import { open as openFolderPicker } from "@tauri-apps/plugin-dialog";
import {
  getEngineInstallation,
  importEngineFolder,
  removeEngineVariant,
  startEngineInstallation,
} from "$lib/ipc/engineInstallation";
import { settingsLoaders } from "$lib/stores/settings";
import { reportError } from "$lib/errors/reportError";
import { createEngineManagement } from "$lib/settings/engineManagement";

/** The folder that holds the archives, the list and its signature, or `null`. */
async function pickFolder(): Promise<string | null> {
  const selected = await openFolderPicker({
    directory: true,
    multiple: false,
    title: get(t)("settings.engines.import_picker_title"),
  });
  return typeof selected === "string" ? selected : null;
}

export const engineManagement = createEngineManagement({
  installation: getEngineInstallation,
  start: startEngineInstallation,
  importFolder: importEngineFolder,
  pickFolder,
  remove: removeEngineVariant,
  reloadEngines: () => settingsLoaders.engines(true),
  wait: (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
  onError: (err) => reportError(err, { surface: "toast" }),
});
