/**
 * The panes of the chat screen, kept while the application runs: leaving the
 * chat for another screen closes no pane, as it releases no model.
 *
 * A pane that stops showing a conversation gives back the lease it held on
 * its model and forgets the load it was refused.
 */
import { derived } from "svelte/store";
import { activeConversation, createChatPanes } from "$lib/chat/chatPanes";
import { paneLeases } from "$lib/chat/modelLease";
import { createPaneLoads } from "$lib/chat/paneLoad";
import { estimateModelLoad } from "$lib/ipc/llmLoad";

/** The loads the panes were refused. */
export const paneLoads = createPaneLoads(estimateModelLoad);

/** The panes of the chat. */
export const chatPanes = createChatPanes((paneId) => {
  void paneLeases.drop(paneId);
  paneLoads.dismiss(paneId);
});

/** The conversation of the active pane, `null` when it shows none. */
export const activeSessionId = derived(chatPanes, activeConversation);
