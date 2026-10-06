/**
 * What the guide says of the screen it opens on.
 *
 * One context per visible route, one per sub-screen a screen declares, and
 * one for the first launch. The words live in the interface catalogues, under
 * `guide.context.<id>`: the text that says what the screen is for, the action
 * the guide offers, two lesser ones, and the question the input suggests.
 * This table holds what a catalogue cannot: the screen each action opens, and
 * the page of the embedded documentation shown when no model can answer.
 */
import {
  ONBOARDING_GUIDE_ROUTE,
  type GuideContext,
  type GuideSubscreen,
} from "$lib/stores/guideContext";
import type { Route } from "$lib/stores/navigation";
import { allowedGuideRoute } from "$lib/apolliaGuide/actionButtons";
import { isVisibleRoute, type VisibleRoute } from "$lib/apolliaGuide/guideRoutes";
import { routeMeta } from "$lib/navigation/routeMeta";

/** The name of a context: a visible route, a declared sub-screen, or the first launch. */
export type GuideContextId = VisibleRoute | GuideSubscreen | typeof ONBOARDING_GUIDE_ROUTE;

export interface ScreenContext {
  /** The screen the offered action opens; `null` when the context offers none. */
  action: Route | null;
  /** The screens the two lesser actions open, in catalogue order. */
  secondary: readonly Route[];
  /** Name of the context's page under `/operator-help/screens/` on the documentation site. */
  page: string;
  /**
   * Set when the catalogue names the context itself (`name`): a sub-screen
   * or the first launch. The other contexts carry the name of their route.
   */
  named?: true;
}

/**
 * Every context. The first launch offers no action: its window covers the
 * screens an action would open.
 */
export const SCREEN_CONTEXTS: Record<GuideContextId, ScreenContext> = {
  agents: { action: "chat", secondary: ["library", "inbox"], page: "agents" },
  "agents.fiche": {
    action: "chat",
    secondary: ["authorizations", "journal"],
    page: "agent-sheet",
    named: true,
  },
  batches: { action: "agents", secondary: ["inbox", "journal"], page: "batches" },
  batch: { action: "batches", secondary: ["inbox", "agents"], page: "batch" },
  chat: { action: "agents", secondary: ["llm", "inbox"], page: "chat" },
  inbox: { action: "authorizations", secondary: ["journal", "chat"], page: "to-review" },
  integrations: { action: "agents", secondary: ["authorizations", "library"], page: "connections" },
  llm: { action: "chat", secondary: ["settings", "agents"], page: "ai" },
  library: { action: "agents", secondary: ["integrations", "authorizations"], page: "library" },
  automations: { action: "agents", secondary: ["journal", "inbox"], page: "automations" },
  memory: { action: "agents", secondary: ["settings", "journal"], page: "memory" },
  transcriptions: { action: "chat", secondary: ["llm", "settings"], page: "transcriptions" },
  observability: { action: "journal", secondary: ["agents", "llm"], page: "observability" },
  policy: {
    action: "authorizations",
    secondary: ["integrations", "journal"],
    page: "organisation-policy",
  },
  authorizations: { action: "journal", secondary: ["agents", "inbox"], page: "permissions" },
  journal: { action: "authorizations", secondary: ["agents", "inbox"], page: "activity-log" },
  developer: { action: "settings", secondary: ["llm", "journal"], page: "developer" },
  settings: { action: "llm", secondary: ["authorizations", "journal"], page: "settings" },
  onboarding: { action: null, secondary: [], page: "first-launch", named: true },
};

/** The catalogue keys of the lesser actions, in the order of `secondary`. */
export const SECONDARY_LEAVES = ["secondary.first", "secondary.second"] as const;

/** Every context name. */
export function guideContextIds(): GuideContextId[] {
  return Object.keys(SCREEN_CONTEXTS) as GuideContextId[];
}

/**
 * The context of where the user is: the declared sub-screen when there is
 * one, the route otherwise. A route outside the visible ones (the design
 * reference of a development build) has no context.
 */
export function guideContextId(context: GuideContext): GuideContextId | null {
  if (context.subscreen) return context.subscreen;
  if (context.route === ONBOARDING_GUIDE_ROUTE) return ONBOARDING_GUIDE_ROUTE;
  return isVisibleRoute(context.route) ? context.route : null;
}

/** The catalogue key of one text of a context. */
export function guideContextKey(id: GuideContextId, leaf: string): string {
  return `guide.context.${id}.${leaf}`;
}

/** The route of a context's page in the embedded documentation and on the site. */
export function guideDocumentationRoute(id: GuideContextId): string {
  return `/operator-help/screens/${SCREEN_CONTEXTS[id].page}`;
}

/** The route a context belongs to: the screen a sub-screen is opened in. */
export function guideContextRoute(id: GuideContextId): VisibleRoute | null {
  if (id === ONBOARDING_GUIDE_ROUTE) return null;
  const route = id.split(".")[0];
  return isVisibleRoute(route) ? route : null;
}

export type Translate = (key: string, options?: { values?: Record<string, string> }) => string;

/** An action the guide offers: its words, and the screen it opens. */
export interface GuideAction {
  label: string;
  /** What the action leads to, said in one sentence; the offered action only. */
  result: string;
  route: Route;
}

/** What the Guide tab shows of a context, in the language of `translate`. */
export interface GuideScreenView {
  id: GuideContextId;
  /** Where the user is, from the screen down to what is open in it. */
  trail: string[];
  /** The last segment of the trail: the open object, the sub-screen, or the screen. */
  title: string;
  /** What the screen is for. */
  text: string;
  /** The offered action, `null` when the context has none or its screen is out of reach. */
  action: GuideAction | null;
  /** The lesser actions whose screen is within reach. */
  secondary: GuideAction[];
  /** The question the input suggests. */
  question: string;
}

/**
 * The view of a context. An action is kept only when the one allowlist lets
 * its screen through, so the tab never offers a screen the router would not
 * show (a developer screen with the mode off).
 */
export function guideScreenView(
  context: GuideContext,
  translate: Translate,
  developerMode: boolean,
): GuideScreenView | null {
  const id = guideContextId(context);
  if (!id) return null;
  const declared = SCREEN_CONTEXTS[id];
  const key = (leaf: string) => guideContextKey(id, leaf);

  const trail: string[] = [];
  const route = guideContextRoute(id);
  if (route) trail.push(translate(routeMeta[route].labelKey));
  if (context.object) trail.push(context.object.name);
  else if (declared.named) trail.push(translate(key("name")));

  const within = (target: Route) => allowedGuideRoute(`/${target}`, developerMode) !== null;
  const name = context.object?.name ?? translate("guide.unnamed_item");
  return {
    id,
    trail,
    title: trail.at(-1) ?? "",
    text: translate(key("text"), { values: { name } }),
    action:
      declared.action && within(declared.action)
        ? {
            label: translate(key("action.label")),
            result: translate(key("action.result")),
            route: declared.action,
          }
        : null,
    secondary: declared.secondary.flatMap((target, index) =>
      within(target)
        ? [{ label: translate(key(SECONDARY_LEAVES[index])), result: "", route: target }]
        : [],
    ),
    question: translate(key("question")),
  };
}

/**
 * The instructions the guide's session receives for a context, in the
 * language of the interface: what the screen is for, then the action the
 * guide may offer with the route it opens.
 */
export function guideContextPrompt(context: GuideContext, translate: Translate): string {
  const id = guideContextId(context);
  if (!id) return "";
  const name = context.object?.name ?? translate("guide.unnamed_item");
  const lines = [translate(guideContextKey(id, "text"), { values: { name } })];
  const { action } = SCREEN_CONTEXTS[id];
  if (action) {
    lines.push(`${translate(guideContextKey(id, "action.label"))} (/${action})`);
  }
  return lines.join("\n");
}
