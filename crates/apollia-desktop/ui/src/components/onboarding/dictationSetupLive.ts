/**
 * The dictation setup wired to the application: one object for the life of
 * the window, so a download started in the first launch ends with dictation
 * switched on even after the step is left.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { cancelModelDownload, getDictationModels } from "$lib/ipc/modelHub";
import { importModelFile, startModelDownload, type DownloadProgress } from "$lib/ipc/models";
import { reloadStt, updateSttConfig } from "$lib/ipc/stt";
import { settingsLoaders } from "$lib/stores/settings";
import { refreshSttStatus } from "$lib/stores/stt";
import type { SttConfigView, SttModelInfo } from "$lib/types";
import { createEmbeddingsSetup } from "./embeddingsSetup";
import {
  DICTATION_EXTENSIONS,
  asSetupModels,
  dictationDownload,
  modelInUse,
  useDictationModel,
} from "./dictationSetup";

const readConfig = (): Promise<SttConfigView> => invoke<SttConfigView>("get_stt_config");
const readInstalled = (): Promise<SttModelInfo[]> => invoke<SttModelInfo[]>("list_stt_models");

export const dictationSetup = createEmbeddingsSetup({
  recommend: async () => asSetupModels(await getDictationModels()),
  current: async () => {
    const [config, installed, models] = await Promise.all([
      readConfig(),
      readInstalled(),
      getDictationModels(),
    ]);
    return modelInUse(config, installed, models.models);
  },
  startDownload: (request) => startModelDownload(dictationDownload(request)),
  cancelDownload: cancelModelDownload,
  importFile: (sourcePath) => importModelFile(sourcePath, DICTATION_EXTENSIONS),
  setDefault: async (model) => {
    const used = await useDictationModel(model, {
      models: getDictationModels,
      installed: readInstalled,
      config: readConfig,
      save: updateSttConfig,
      reload: reloadStt,
    });
    // What the settings and the test below read is read again: the model
    // and the switch just changed under them.
    await Promise.all([
      settingsLoaders.sttConfig(true),
      settingsLoaders.sttModels(true),
      refreshSttStatus(),
    ]).catch(() => undefined);
    return used;
  },
  onProgress: (handler) =>
    listen<DownloadProgress>("model-download-progress", (event) => handler(event.payload)),
});
