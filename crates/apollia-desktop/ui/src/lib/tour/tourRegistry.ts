/**
 * Registration of all named guided tours used by the onboarding and
 * post-onboarding surfaces. Importing this module has a side effect:
 * all tours are added to the {@link ./GuidedTour.ts} registry.
 */
import { registerTour, type TourDefinition } from "./GuidedTour";

// TODO: selectors below are best-guesses; validate against the
// actual DOM in a follow-up pass once the targets have `data-testid` hooks.

const firstChat: TourDefinition = {
  id: "first_chat",
  steps: [
    {
      id: "fc-1",
      targetSelector: '[data-testid="chat-input"]',
      titleKey: "tours.first_chat.step_1_title",
      bodyKey: "tours.first_chat.step_1_body",
    },
    {
      id: "fc-2",
      targetSelector: '[data-testid="chat-send-button"]',
      titleKey: "tours.first_chat.step_2_title",
      bodyKey: "tours.first_chat.step_2_body",
    },
    {
      id: "fc-3",
      targetSelector: '[data-testid="chat-thinking-panel"]',
      titleKey: "tours.first_chat.step_3_title",
      bodyKey: "tours.first_chat.step_3_body",
    },
  ],
};

const firstAutomation: TourDefinition = {
  id: "first_automation",
  steps: [
    {
      id: "fa-1",
      targetSelector: '[data-testid="automations-create-button"]',
      titleKey: "tours.first_automation.step_1_title",
      bodyKey: "tours.first_automation.step_1_body",
    },
    {
      id: "fa-2",
      targetSelector: '[data-testid="automation-trigger-config"]',
      titleKey: "tours.first_automation.step_2_title",
      bodyKey: "tours.first_automation.step_2_body",
    },
    {
      id: "fa-3",
      targetSelector: '[data-testid="automation-save-button"]',
      titleKey: "tours.first_automation.step_3_title",
      bodyKey: "tours.first_automation.step_3_body",
    },
  ],
};

const firstMemory: TourDefinition = {
  id: "first_memory",
  steps: [
    {
      id: "fm-1",
      targetSelector: '[data-testid="memory-entries-list"]',
      titleKey: "tours.first_memory.step_1_title",
      bodyKey: "tours.first_memory.step_1_body",
    },
    {
      id: "fm-2",
      targetSelector: '[data-testid="memory-add-entry"]',
      titleKey: "tours.first_memory.step_2_title",
      bodyKey: "tours.first_memory.step_2_body",
    },
  ],
};

const firstInbox: TourDefinition = {
  id: "first_inbox",
  steps: [
    {
      id: "fi-1",
      targetSelector: '[data-testid="inbox-list"]',
      titleKey: "tours.first_inbox.step_1_title",
      bodyKey: "tours.first_inbox.step_1_body",
    },
    {
      id: "fi-2",
      targetSelector: '[data-testid="inbox-preview"]',
      titleKey: "tours.first_inbox.step_2_title",
      bodyKey: "tours.first_inbox.step_2_body",
    },
    {
      id: "fi-3",
      // TODO: confirm selector once approval buttons are reworked.
      targetSelector: '[data-testid="inbox-approve-button"]',
      titleKey: "tours.first_inbox.step_3_title",
      bodyKey: "tours.first_inbox.step_3_body",
    },
  ],
};

/** Register all named tours. Idempotent — safe to call multiple times. */
export function registerAllTours(): void {
  registerTour(firstChat);
  registerTour(firstAutomation);
  registerTour(firstMemory);
  registerTour(firstInbox);
}

// Side effect on import: wire the registry immediately.
registerAllTours();
