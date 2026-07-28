/**
 * The tour catalogue.
 *
 * Every anchor below was checked against the current UI. Nothing here points at
 * a capability the product does not ship: creating a task from the interface,
 * adding a memory entry, installing a useful assistant, autonomy tiers and the
 * memory-insight review are all absent or dead, and are deliberately not toured.
 *
 * Vocabulary follows `lib/i18n/operator-glossary.md`: assistant rather than
 * agent, automation rather than trigger.
 */
import { openNewChatRequested } from "$lib/stores/chat";
import type { TourDefinition, TourId } from "./types";

/**
 * Act 1. The first real result, annotated live.
 *
 * Presented without a scrim and without a focus trap: the second annotation
 * points at a live approval card, and the user has to be able to answer it.
 *
 * The approval annotation waits for its anchor with no budget. Depending on the
 * question asked and on the rules already granted during onboarding, the card
 * may never appear. That silence is accepted: no textual fallback fires, the
 * tour simply moves on.
 */
const FIRST_RESULT: TourDefinition = {
  id: "first-result",
  presentation: "annotated",
  route: "chat",
  titleKey: "tour.first_result.title",
  // Opens the new-conversation picker so the user lands on a blank composer
  // with the starter templates in reach, rather than on an empty session list.
  // Reuses the store the sidebar button and the command palette already write
  // to; the tour adds no channel of its own.
  onStart: () => openNewChatRequested.set(Date.now()),
  steps: [
    {
      // The picker opens on the next tick, hence the wait. Optional so a tour
      // started from an already-open conversation degrades instead of failing.
      id: "compose",
      anchor: { kind: "testid", value: "quickpicker-textarea" },
      titleKey: "tour.first_result.compose.title",
      bodyKey: "tour.first_result.compose.body",
      optional: true,
      awaitAnchor: true,
    },
    {
      id: "approval",
      anchor: { kind: "testidPrefix", value: "operator-approval-" },
      titleKey: "tour.first_result.approval.title",
      bodyKey: "tour.first_result.approval.body",
      optional: true,
      awaitAnchor: true,
    },
    {
      id: "trace",
      anchor: { kind: "testidPrefix", value: "chat-message-", nth: -1 },
      titleKey: "tour.first_result.trace.title",
      bodyKey: "tour.first_result.trace.body",
      optional: true,
    },
  ],
};

/**
 * Act 2. The landmarks.
 *
 * Four anchors, all in the permanent chrome and present in both Operator and
 * Builder. No route change: the previous tour pushed slash-prefixed routes into
 * a slug-typed store and unmounted the whole page body.
 */
const LANDMARKS: TourDefinition = {
  id: "landmarks",
  presentation: "modal",
  route: null,
  titleKey: "tour.landmarks.title",
  steps: [
    {
      id: "surfaces",
      anchor: { kind: "testid", value: "sidebar" },
      titleKey: "tour.landmarks.surfaces.title",
      bodyKey: "tour.landmarks.surfaces.body",
    },
    {
      id: "palette",
      anchor: { kind: "testid", value: "topbar-search" },
      titleKey: "tour.landmarks.palette.title",
      bodyKey: "tour.landmarks.palette.body",
    },
    {
      id: "modes",
      anchor: { kind: "testid", value: "sidebar-mode-chip" },
      titleKey: "tour.landmarks.modes.title",
      bodyKey: "tour.landmarks.modes.body",
    },
    {
      id: "inbox",
      anchor: { kind: "testid", value: "topbar-inbox" },
      titleKey: "tour.landmarks.inbox.title",
      bodyKey: "tour.landmarks.inbox.body",
    },
  ],
};

/**
 * Act 3, milestone "frame".
 *
 * The causal link between attaching a document and the assistant's prompt is
 * invisible today: the project context is injected silently. The last step is
 * the whole point of the micro-tour.
 */
const FRAME: TourDefinition = {
  id: "frame",
  presentation: "modal",
  route: "projects",
  titleKey: "tour.frame.title",
  steps: [
    {
      id: "what",
      anchor: { kind: "testid", value: "projects-page" },
      titleKey: "tour.frame.what.title",
      bodyKey: "tour.frame.what.body",
    },
    {
      // The context tab is where documents and instructions live. Anchoring on
      // the tab rather than on a documents pane keeps the step resolvable
      // whether or not a project is currently selected.
      id: "documents",
      anchor: { kind: "testid", value: "project-tab-context" },
      titleKey: "tour.frame.documents.title",
      bodyKey: "tour.frame.documents.body",
      optional: true,
    },
    {
      id: "context",
      anchor: null,
      titleKey: "tour.frame.context.title",
      bodyKey: "tour.frame.context.body",
    },
  ],
};

/**
 * Act 3, milestone "automate".
 *
 * The wizard's natural-language parsing is heuristic, with no model involved,
 * and the copy must not suggest otherwise. Webhooks are not toured: the API
 * listens on loopback by default, so they are unusable without a tunnel.
 */
const AUTOMATE: TourDefinition = {
  id: "automate",
  presentation: "modal",
  route: "automations",
  titleKey: "tour.automate.title",
  steps: [
    {
      id: "create",
      anchor: { kind: "testid", value: "automations-new-btn" },
      titleKey: "tour.automate.create.title",
      bodyKey: "tour.automate.create.body",
    },
    {
      id: "when",
      anchor: { kind: "testid", value: "automation-wizard-schedule-card" },
      titleKey: "tour.automate.when.title",
      bodyKey: "tour.automate.when.body",
      optional: true,
    },
    {
      id: "run-now",
      anchor: { kind: "testid", value: "automations-table" },
      titleKey: "tour.automate.run_now.title",
      bodyKey: "tour.automate.run_now.body",
      optional: true,
    },
  ],
};

/**
 * Act 3, milestone "connect".
 *
 * Lives on the connections route rather than inside the connector wizard: the
 * previous first-connection tour ran while the wizard dialog was closed and
 * pointed at a sheet that was never open.
 */
const CONNECT: TourDefinition = {
  id: "connect",
  presentation: "modal",
  route: "integrations",
  titleKey: "tour.connect.title",
  steps: [
    {
      id: "catalogue",
      anchor: { kind: "testid", value: "connections-open-catalogue" },
      titleKey: "tour.connect.catalogue.title",
      bodyKey: "tour.connect.catalogue.body",
    },
    {
      id: "list",
      anchor: { kind: "testid", value: "connections-sidebar-list" },
      titleKey: "tour.connect.list.title",
      bodyKey: "tour.connect.list.body",
    },
    {
      id: "capabilities",
      anchor: { kind: "testid", value: "native-capabilities" },
      titleKey: "tour.connect.capabilities.title",
      bodyKey: "tour.connect.capabilities.body",
      optional: true,
    },
  ],
};

/**
 * Act 3, milestone "follow".
 *
 * The tamper-evident journal is the most defensible capability of the product
 * and sits several clicks deep, so the tour points at it.
 *
 * It stops at the journal tab and does not promise the verification action:
 * `AuditVerifyButton` only renders when a `runId` is supplied, and
 * `Observability.svelte` never supplies one because `AuditTrailEntry` carries no
 * run identifier. Pointing at a button that cannot appear is the exact failure
 * this rework exists to remove; surfacing the run id is backend work and
 * belongs to its own change.
 */
const FOLLOW: TourDefinition = {
  id: "follow",
  presentation: "modal",
  route: "observability",
  titleKey: "tour.follow.title",
  steps: [
    {
      id: "timeline",
      anchor: { kind: "testid", value: "observability-tabbar" },
      titleKey: "tour.follow.timeline.title",
      bodyKey: "tour.follow.timeline.body",
    },
    {
      id: "costs",
      anchor: { kind: "testid", value: "observability-tab-llm-costs" },
      titleKey: "tour.follow.costs.title",
      bodyKey: "tour.follow.costs.body",
      optional: true,
    },
    {
      id: "journal",
      anchor: { kind: "testid", value: "observability-tab-audit-trail" },
      titleKey: "tour.follow.journal.title",
      bodyKey: "tour.follow.journal.body",
      optional: true,
    },
  ],
};

/** Every tour, by identifier. */
export const TOURS: Readonly<Record<TourId, TourDefinition>> = {
  "first-result": FIRST_RESULT,
  landmarks: LANDMARKS,
  frame: FRAME,
  automate: AUTOMATE,
  connect: CONNECT,
  follow: FOLLOW,
};

/** Returns a tour definition by identifier. */
export function tourById(id: TourId): TourDefinition {
  return TOURS[id];
}
