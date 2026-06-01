/**
 * Empty-state string catalogue.
 *
 * Each variant describes how to build a canonical `<EmptyState>` for a given
 * scope: the i18n keys to pull, the Lucide icon to render in the default
 * illustration, and the `page` test-id used by E2E coverage.
 *
 * The actual copies live in `en.json` / `fr.json` under `empty_states.*`
 * - this file is the typed index that the design showcase iterates over and
 * that route components reference to stay in sync with the catalogue.
 */
import type { Icon } from "lucide-svelte";
import {
  Bell,
  Bot,
  ListTodo,
  MessageSquare,
  Plug,
  Timer,
} from "lucide-svelte";

/** Canonical scopes covered by the empty-state catalogue. */
export type EmptyStateVariant =
  | "agents"
  | "tasks"
  | "triggers"
  | "chat"
  | "notifications"
  | "integrations";

export interface EmptyStateEntry {
  /** `data-testid` suffix → `empty-state-${page}`. */
  page: EmptyStateVariant;
  /** Default Lucide icon rendered inside the gradient illustration. */
  icon: typeof Icon;
  /** i18n key for the display-sm title. */
  titleKey: string;
  /** i18n key for the body description (1–2 sentences). */
  descriptionKey: string;
  /** i18n key for the primary CTA label. Optional. */
  primaryCtaKey?: string;
  /** i18n key for the secondary CTA label (ghost). Optional. */
  secondaryCtaKey?: string;
}

/** Canonical catalogue - one entry per targeted scope. */
export const EMPTY_STATES: Record<EmptyStateVariant, EmptyStateEntry> = {
  agents: {
    page: "agents",
    icon: Bot,
    titleKey: "empty_states.agents.title",
    descriptionKey: "empty_states.agents.description",
    primaryCtaKey: "empty_states.agents.primary_cta",
    secondaryCtaKey: "empty_states.agents.secondary_cta",
  },
  tasks: {
    page: "tasks",
    icon: ListTodo,
    titleKey: "empty_states.tasks.title",
    descriptionKey: "empty_states.tasks.description",
    primaryCtaKey: "empty_states.tasks.primary_cta",
    secondaryCtaKey: "empty_states.tasks.secondary_cta_operator",
  },
  triggers: {
    page: "triggers",
    icon: Timer,
    titleKey: "empty_states.triggers.title",
    descriptionKey: "empty_states.triggers.description",
    primaryCtaKey: "empty_states.triggers.primary_cta",
    secondaryCtaKey: "empty_states.triggers.secondary_cta",
  },
  chat: {
    page: "chat",
    icon: MessageSquare,
    titleKey: "empty_states.chat.title",
    descriptionKey: "empty_states.chat.description",
    primaryCtaKey: "empty_states.chat.primary_cta",
    secondaryCtaKey: "empty_states.chat.secondary_cta",
  },
  notifications: {
    page: "notifications",
    icon: Bell,
    titleKey: "empty_states.notifications.title",
    descriptionKey: "empty_states.notifications.description",
    primaryCtaKey: "empty_states.notifications.primary_cta",
    secondaryCtaKey: "empty_states.notifications.secondary_cta",
  },
  integrations: {
    page: "integrations",
    icon: Plug,
    titleKey: "empty_states.integrations.title",
    descriptionKey: "empty_states.integrations.description",
    primaryCtaKey: "empty_states.integrations.primary_cta",
    secondaryCtaKey: "empty_states.integrations.secondary_cta",
  },
};

/** Ordered list - drives the design showcase iteration. */
export const EMPTY_STATE_ORDER: EmptyStateVariant[] = [
  "agents",
  "tasks",
  "triggers",
  "chat",
  "notifications",
  "integrations",
];
