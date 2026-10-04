/**
 * Where the user is, as the guide reads it: the route, the sub-screen the
 * screen declares (a record opened inside it), and the object that record
 * shows. The onboarding modal overrides all three with `onboarding`.
 *
 * A screen declares its sub-screen with `declareGuideSubscreen` from an
 * effect and returns the cleanup it gets back, so the declaration ends when
 * the record closes or the screen unmounts.
 */
import { derived, writable, type Readable } from "svelte/store";
import { currentRoute } from "$lib/stores/navigation";
import { onboardingModalOpen } from "$lib/stores/onboarding";

/** Every sub-screen a screen declares. One context text exists per entry. */
export const GUIDE_SUBSCREENS = ["agents.fiche"] as const;

export type GuideSubscreen = (typeof GUIDE_SUBSCREENS)[number];

/** Context name the guide receives while the onboarding modal is open. */
export const ONBOARDING_GUIDE_ROUTE = "onboarding";

/** The record a sub-screen shows, so the guide can name it. */
export interface GuideObject {
  id: string;
  name: string;
}

export interface GuideContext {
  /** Application route, or `onboarding` while the onboarding modal is open. */
  route: string;
  /** Sub-screen declared by the screen, `null` on the screen itself. */
  subscreen: GuideSubscreen | null;
  /** Object the sub-screen shows, `null` when none. */
  object: GuideObject | null;
}

interface Declaration {
  subscreen: GuideSubscreen;
  object: GuideObject | null;
}

const declared = writable<Declaration | null>(null);

/**
 * Declares the sub-screen the current screen shows. Returns the cleanup that
 * withdraws it; a later declaration replaces an earlier one, and the cleanup
 * of the earlier one then leaves the later one in place.
 */
export function declareGuideSubscreen(
  subscreen: GuideSubscreen,
  object: GuideObject | null = null,
): () => void {
  const declaration: Declaration = { subscreen, object };
  declared.set(declaration);
  return () => {
    declared.update((current) => (current === declaration ? null : current));
  };
}

/** Resolves the context from the route, the onboarding flag and a declaration. */
export function resolveGuideContext(
  route: string,
  onboarding: boolean,
  declaration: Declaration | null,
): GuideContext {
  if (onboarding) {
    return { route: ONBOARDING_GUIDE_ROUTE, subscreen: null, object: null };
  }
  return {
    route,
    subscreen: declaration?.subscreen ?? null,
    object: declaration?.object ?? null,
  };
}

/** Stable identity of a context, to skip a refresh that changes nothing. */
export function guideContextKey(context: GuideContext): string {
  return [context.route, context.subscreen ?? "", context.object?.id ?? ""].join("|");
}

/**
 * The lines appended to the guide's instructions after the route's context
 * text. They are read by the model, not by the user, so they stay in English
 * like the rest of the guide's instructions.
 */
export function guidePromptDetails(context: GuideContext): string {
  const lines: string[] = [];
  // i18n-ignore-start: model-facing instructions, English by design
  if (context.subscreen) lines.push(`Current sub-screen: ${context.subscreen}.`);
  if (context.object) {
    lines.push(`Open item: "${context.object.name}" (id: ${context.object.id}).`);
  }
  // i18n-ignore-end
  return lines.join("\n");
}

export const guideContext: Readable<GuideContext> = derived(
  [currentRoute, onboardingModalOpen, declared],
  ([$route, $onboarding, $declared]) =>
    resolveGuideContext($route, $onboarding, $declared),
);

/** Withdraws any declaration (tests). */
export function resetGuideSubscreen(): void {
  declared.set(null);
}
