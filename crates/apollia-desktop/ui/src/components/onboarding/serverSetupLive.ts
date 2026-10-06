/**
 * The server setup wired to the application: one object for the life of the
 * window, so a server chosen in the first launch is still the chosen one when
 * the person comes back to the step.
 */
import { saveLlmServer, setDefaultModel, testLlmServer } from "$lib/ipc/llmServers";
import { createServerSetup } from "./serverSetup";

export const serverSetup = createServerSetup({
  test: testLlmServer,
  save: saveLlmServer,
  setDefault: setDefaultModel,
});
