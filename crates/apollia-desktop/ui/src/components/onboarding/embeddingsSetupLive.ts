/**
 * The embeddings setup wired to the application: one object for the life of
 * the window, so a download started in the onboarding ends as the default
 * model even after the step is left.
 */
import { listen } from "@tauri-apps/api/event";
import { getEmbeddingsRecommendation, setDefaultEmbeddingsModel } from "$lib/ipc/embeddings";
import { getMemoryEmbeddingsModel } from "$lib/ipc/memory";
import { cancelModelDownload, startVariantDownload } from "$lib/ipc/modelHub";
import { importModelFile, type DownloadProgress } from "$lib/ipc/models";
import { createEmbeddingsSetup } from "./embeddingsSetup";

export const embeddingsSetup = createEmbeddingsSetup({
  recommend: getEmbeddingsRecommendation,
  current: getMemoryEmbeddingsModel,
  startDownload: startVariantDownload,
  cancelDownload: cancelModelDownload,
  importFile: importModelFile,
  setDefault: setDefaultEmbeddingsModel,
  onProgress: (handler) =>
    listen<DownloadProgress>("model-download-progress", (event) => handler(event.payload)),
});
