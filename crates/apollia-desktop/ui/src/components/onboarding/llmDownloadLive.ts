/**
 * The language-model download wired to the application: one object for the
 * life of the window, so a download started at the first launch ends as the
 * default engine after its step is left, and after the modal is closed.
 */
import { listen } from "@tauri-apps/api/event";
import {
  cancelModelDownload,
  reloadLlm,
  scanForGgufModels,
  setupLocalLlm,
  type DownloadProgress,
} from "$lib/ipc/models";
import { refreshLlmBackends } from "$lib/stores/sse";
import { createLlmDownload } from "./llmDownload";

export const llmDownload = createLlmDownload({
  cancelDownload: cancelModelDownload,
  scan: scanForGgufModels,
  wire: async (path) => {
    await setupLocalLlm(path);
    await reloadLlm();
    // No event tells the backend list an engine was wired here; without this
    // the conversation step would wait for the next periodic refresh.
    await refreshLlmBackends();
  },
  onProgress: (handler) =>
    listen<DownloadProgress>("model-download-progress", (event) => handler(event.payload)),
});
