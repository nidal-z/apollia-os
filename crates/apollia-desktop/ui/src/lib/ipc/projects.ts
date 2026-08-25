// Typed IPC wrappers for the Projects surface.
//
// The route consumes several backend commands: the project CRUD proper
// (`list_projects`, `get_project`, `create_project`, `update_project`,
// `delete_project`, `suggest_workspace_path`, `set_project_provider`) plus a
// few cross-domain reads the detail pane aggregates per project (chats, tasks,
// memory namespaces) and one write (`create_chat_session`). Centralizing them
// here keeps `Projects.svelte` and its child components free of raw `invoke`
// calls and gives every call site a single typed signature.

import { invoke } from "@tauri-apps/api/core";
import type {
  ChatSessionSummary,
  CreateProjectRequest,
  CreateSessionRequest,
  MemoryEntry,
  ProjectDetail,
  ProjectDocument,
  ProjectSummary,
  TaskSummary,
  UpdateProjectRequest,
  WorkspaceSnapshotView,
} from "$lib/types";

/** List every project (sidebar master list). */
export function listProjects(): Promise<ProjectSummary[]> {
  return invoke<ProjectSummary[]>("list_projects");
}

/** Full detail for a single project (right pane). */
export function getProject(id: string): Promise<ProjectDetail> {
  return invoke<ProjectDetail>("get_project", { id });
}

/** Create a project from the New Project dialog. */
export function createProject(
  request: CreateProjectRequest,
): Promise<ProjectSummary> {
  return invoke<ProjectSummary>("create_project", { request });
}

/** Partial update of a project's editable metadata. */
export function updateProject(
  id: string,
  request: UpdateProjectRequest,
): Promise<ProjectDetail> {
  return invoke<ProjectDetail>("update_project", { id, request });
}

/** Permanently delete a project. */
export function deleteProject(id: string): Promise<void> {
  return invoke<void>("delete_project", { id });
}

/** Suggest a workspace path derived from the project name. */
export function suggestWorkspacePath(projectName: string): Promise<string> {
  return invoke<string>("suggest_workspace_path", { projectName });
}

/** Arguments for seeding a context provider (developer template). */
export interface SetProjectProviderArgs {
  providerId: string | null;
  projectId: string;
  providerType: string;
  name: string;
  configJson: string;
  path: string | null;
  enabled: boolean;
  priority: number;
}

/** Create or update a context provider attached to a project. */
export function setProjectProvider(args: SetProjectProviderArgs): Promise<void> {
  return invoke<void>("set_project_provider", { ...args });
}

/**
 * Attach a local file to a project.
 *
 * `filePath` comes from the native file picker, the backend reads the size and
 * the base name from disk and returns the freshly created row.
 */
export function uploadProjectDocument(
  projectId: string,
  filePath: string,
): Promise<ProjectDocument> {
  return invoke<ProjectDocument>("upload_project_document", {
    projectId,
    filePath,
  });
}

/** Detach a document from its project (the file on disk is left alone). */
export function deleteProjectDocument(docId: string): Promise<void> {
  return invoke<void>("delete_project_document", { docId });
}

/** Attach an installed agent to a project. */
export function addProjectAgent(
  projectId: string,
  agentName: string,
): Promise<void> {
  return invoke<void>("add_project_agent", { projectId, agentName });
}

/** Detach an agent from a project. */
export function removeProjectAgent(
  projectId: string,
  agentName: string,
): Promise<void> {
  return invoke<void>("remove_project_agent", { projectId, agentName });
}

/** Names of the agents attached to a project. */
export function listProjectAgents(projectId: string): Promise<string[]> {
  return invoke<string[]>("list_project_agents", { projectId });
}

/** Chats scoped to a project (Conversations tab). */
export function listChatsByProject(
  projectId: string,
): Promise<ChatSessionSummary[]> {
  return invoke<ChatSessionSummary[]>("list_chats_by_project", { projectId });
}

/** All tasks (the Tasks tab narrows them by the project's agents). */
export function listTasks(
  filter: unknown = null,
): Promise<TaskSummary[]> {
  return invoke<TaskSummary[]>("list_tasks", { filter });
}

/** Every memory namespace (the Memory tab keeps the `{project_id}:*` ones). */
export function listMemoryNamespaces(): Promise<string[]> {
  return invoke<string[]>("list_memory_namespaces");
}

/** Entries within a single memory namespace. */
export function listMemoryEntries(namespace: string): Promise<MemoryEntry[]> {
  return invoke<MemoryEntry[]>("list_memory_entries", { namespace });
}

/** Open a new chat session bound to a project. */
export function createChatSession(
  request: CreateSessionRequest,
): Promise<ChatSessionSummary> {
  return invoke<ChatSessionSummary>("create_chat_session", { request });
}

/** Filesystem snapshot of a project workspace (context providers tab). */
export function getProjectSnapshot(projectId: string): Promise<WorkspaceSnapshotView> {
  return invoke<WorkspaceSnapshotView>("get_project_snapshot", { projectId });
}

/** Remove a context provider from its project. */
export function removeProjectProvider(providerId: string): Promise<void> {
  return invoke<void>("remove_project_provider", { providerId });
}

/** Enable or disable a context provider without removing it. */
export function toggleProjectProvider(
  providerId: string,
  enabled: boolean,
): Promise<void> {
  return invoke<void>("toggle_project_provider", { providerId, enabled });
}
