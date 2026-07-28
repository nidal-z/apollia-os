/**
 * Getting-started milestones.
 *
 * Milestones reflect what the user has actually done, not how many times they
 * clicked Next in a tour. Four of them read a live store; "follow" reads the one
 * persisted flag, because consulting the activity journal is an act rather than
 * an object and no store can report it.
 *
 * "Connect" is satisfied by either a local MCP server or a native OAuth
 * connector. Requiring the OAuth path alone would make the milestone
 * unreachable for anyone unwilling to register their own Google client.
 */
import { derived, writable, type Readable } from "svelte/store";
import { chatSessions, triggers } from "$lib/stores/sse";
import { projects } from "$lib/stores/projects";
import { listMcpServers, oauthGetStatus } from "$lib/ipc/connections";
import { tourState } from "./persistence";
import type { MilestoneId, TourId } from "./types";

/** A single milestone, as rendered by the getting-started band. */
export interface Milestone {
  readonly id: MilestoneId;
  /** Tour opened by the "show me" affordance. */
  readonly tour: TourId;
  readonly done: boolean;
}

/** True once a connector is set up. Refreshed by {@link refreshConnected}. */
const connected = writable(false);

/**
 * Re-reads the connector state.
 *
 * There is no global connector store: the connections route loads its own data.
 * The band asks for it explicitly rather than duplicating that state.
 */
export async function refreshConnected(): Promise<void> {
  try {
    const [servers, accounts] = await Promise.all([listMcpServers(), oauthGetStatus()]);
    connected.set(servers.length > 0 || accounts.length > 0);
  } catch {
    // A connector probe that fails must not blank an otherwise valid band.
    // Leaving the previous value is the least surprising outcome.
  }
}

/** The five milestones, in the order the band renders them. */
export const milestones: Readable<readonly Milestone[]> = derived(
  [chatSessions, projects, triggers, connected, tourState],
  ([$sessions, $projects, $triggers, $connected, $state]) => [
    { id: "chat" as const, tour: "first-result" as const, done: $sessions.length > 0 },
    { id: "frame" as const, tour: "frame" as const, done: $projects.length > 0 },
    { id: "automate" as const, tour: "automate" as const, done: $triggers.length > 0 },
    { id: "connect" as const, tour: "connect" as const, done: $connected },
    { id: "follow" as const, tour: "follow" as const, done: $state.followVisited },
  ],
);

/** How many milestones are done. */
export const completedCount: Readable<number> = derived(milestones, ($milestones) =>
  $milestones.filter((milestone) => milestone.done).length,
);

/** True once every milestone is done. */
export const allComplete: Readable<boolean> = derived(
  milestones,
  ($milestones) => $milestones.every((milestone) => milestone.done),
);

/**
 * The next milestone to suggest, or `null` when everything is done.
 *
 * The band's call to action follows this rather than disappearing after the
 * first milestone, so there is always one obvious way in.
 */
export const nextMilestone: Readable<Milestone | null> = derived(
  milestones,
  ($milestones) => $milestones.find((milestone) => !milestone.done) ?? null,
);
