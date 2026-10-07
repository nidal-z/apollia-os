/**
 * The servers of the AI screen wired to the application: one object for the
 * life of the window, so the column and the content read the same servers.
 */
import {
  deleteLlmServer,
  getDefaultModel,
  listLlmServerModels,
  listLlmServers,
  saveLlmServer,
  setDefaultModel,
  testLlmServer,
  updateLlmServer,
} from "$lib/ipc/llmServers";
import { reloadLlmFromDb } from "$lib/ipc/llm";
import { createServers } from "./servers";

export const servers = createServers({
  list: listLlmServers,
  models: listLlmServerModels,
  test: testLlmServer,
  save: saveLlmServer,
  update: updateLlmServer,
  remove: deleteLlmServer,
  getDefault: getDefaultModel,
  setDefault: setDefaultModel,
  reloadRouter: reloadLlmFromDb,
});
