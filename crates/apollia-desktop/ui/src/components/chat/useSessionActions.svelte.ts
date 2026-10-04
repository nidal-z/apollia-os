/**
 * What the conversation header and the slash commands do to a session:
 * rename it, export it, delete it.
 *
 * None of it touches the message thread, which is why it sits apart from the
 * conversation itself. `refresh` is the one thing it asks back: a rename changes
 * what the backend stores about the session, and the header reads that from the
 * reloaded detail.
 */
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { addToast } from "$lib/components/ui/toast/store";
import { exportConversation, type ExportFormat } from "$lib/chat/exportConversation";
import {
  exportConversation as ipcExportConversation,
  getChatSession,
  renameChatSession,
} from "$lib/ipc/chat";

export interface SessionActions {
  /**
   * Bumped by `/rename` to ask the header to open its inline title editor. The
   * browser `prompt()` is unavailable in the Tauri webview, so `/rename` used to
   * silently no-op; the inline editor is the canonical rename path.
   */
  readonly renameTrigger: number;
  runSlashCommand(cmdId: "export" | "rename"): Promise<void>;
  exportSession(format: ExportFormat): Promise<void>;
  rename(title: string): Promise<void>;
}

export function createSessionActions(
  sessionId: () => string,
  refresh: () => Promise<void>,
): SessionActions {
  let renameTrigger = $state(0);

  async function exportSession(format: ExportFormat): Promise<void> {
    try {
      const detail = await getChatSession(sessionId());
      const { content, filename, mime } = exportConversation(detail, format);
      const dest = await saveDialog({
        defaultPath: filename,
        filters: [{
          name: format === "json" ? "JSON" : "Markdown",
          extensions: [format === "json" ? "json" : "md"],
        }],
      });
      if (!dest) return;
      await ipcExportConversation(dest, content, mime);
    } catch (err) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    }
  }

  return {
    get renameTrigger() {
      return renameTrigger;
    },

    async runSlashCommand(cmdId: "export" | "rename"): Promise<void> {
      switch (cmdId) {
        case "rename":
          renameTrigger++;
          return;
        case "export":
          await exportSession("markdown-with-tools");
          return;
      }
    },

    exportSession,

    async rename(title: string): Promise<void> {
      try {
        await renameChatSession(sessionId(), title);
        void refresh();
      } catch (err) {
        console.warn("rename_chat_session failed", err);
      }
    },
  };
}
