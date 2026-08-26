/**
 * What the conversation header and the slash commands do to a session:
 * rename it, export it, delete it, and link it to a project.
 *
 * None of it touches the message thread, which is why it sits apart from the
 * conversation itself. `refresh` is the one thing it asks back: three of these
 * actions change what the backend stores about the session, and the header reads
 * that from the reloaded detail.
 */
import { get } from "svelte/store";
import { t } from "svelte-i18n";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { addToast } from "$lib/components/ui/toast/store";
import { exportConversation, type ExportFormat } from "$lib/chat/exportConversation";
import {
  exportConversation as ipcExportConversation,
  getChatSession,
  linkChatToProject,
  listChatSessions,
  renameChatSession,
} from "$lib/ipc/chat";
import { listProjects } from "$lib/ipc/projects";
import type { ProjectSummary } from "$lib/types";

export interface SessionActions {
  /**
   * Bumped by `/rename` to ask the header to open its inline title editor. The
   * browser `prompt()` is unavailable in the Tauri webview, so `/rename` used to
   * silently no-op; the inline editor is the canonical rename path.
   */
  readonly renameTrigger: number;
  readonly availableProjects: ProjectSummary[];
  /** The project this session is linked to, or `null`. */
  readonly linkedProject: ProjectSummary | null;
  runSlashCommand(cmdId: "export" | "rename"): Promise<void>;
  exportSession(format: ExportFormat): Promise<void>;
  rename(title: string): Promise<void>;
  loadProjects(): Promise<void>;
  link(projectId: string | null): Promise<void>;
  openProjects(): void;
}

export function createSessionActions(
  sessionId: () => string,
  linkedProjectId: () => string | null,
  refresh: () => Promise<void>,
): SessionActions {
  let renameTrigger = $state(0);
  let availableProjects = $state<ProjectSummary[]>([]);

  const linkedProject = $derived<ProjectSummary | null>(
    (() => {
      const pid = linkedProjectId();
      if (!pid) return null;
      return availableProjects.find((p) => p.id === pid) ?? null;
    })(),
  );

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
    get availableProjects() {
      return availableProjects;
    },
    get linkedProject() {
      return linkedProject;
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

    async loadProjects(): Promise<void> {
      try {
        availableProjects = await listProjects();
      } catch (err) {
        console.warn("list_projects failed", err);
      }
    },

    async link(projectId: string | null): Promise<void> {
      try {
        await linkChatToProject(sessionId(), projectId);
        await refresh();
        // Refresh the global chat-sessions store so the sidebar chip updates
        // immediately (link_chat_to_project does not emit a runtime event).
        try {
          const updated = await listChatSessions();
          const { chatSessions } = await import("$lib/stores/sse");
          chatSessions.set(updated);
        } catch { /* non-blocking */ }
        if (projectId) {
          const project = availableProjects.find((p) => p.id === projectId);
          addToast(
            get(t)("chat.project_linked_toast", {
              values: { name: project?.name ?? "" },
            }),
            "success",
          );
        } else {
          addToast(get(t)("chat.project_unlinked_toast"), "success");
        }
      } catch (err) {
        addToast(
          `${get(t)("chat.project_link_failed")} - ${err instanceof Error ? err.message : String(err)}`,
          "error",
        );
      }
    },

    openProjects(): void {
      import("$lib/stores/navigation").then((m) => m.navigateTo("projects"));
    },
  };
}
