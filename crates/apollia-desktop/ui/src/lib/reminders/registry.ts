/**
 * The registry of the dismissable reminders.
 *
 * Every reminder the user can close and later want back is listed here, with
 * its id, the catalogue key of its label (FR and EN entries in the two
 * catalogues), a read of its state and a clear of that state. Settings >
 * Appearance reads this list: a reminder added here shows up there, with its
 * own reset, without touching the section.
 *
 * The order is the order of the rows under "Onboarding reminders".
 */
import { get } from "svelte/store";
import { resetOnboarding } from "$lib/ipc/danger";
import { onboardingStore, onboardingModalOpen } from "$lib/stores/onboarding";
import { nextSteps, NEXT_STEPS_REMINDER_ID } from "$lib/stores/nextSteps";

export interface Reminder {
  /** Stable id; also the suffix of `data-testid` on its row. */
  readonly id: string;
  /** i18n key of the label shown on its row. */
  readonly labelKey: string;
  /** Whether the user has dismissed it, so a reset would bring it back. */
  isDismissed(): Promise<boolean>;
  /** Clear its state, so it shows again. */
  reset(): Promise<void>;
}

export const ONBOARDING_REMINDER_ID = "onboarding";

const onboarding: Reminder = {
  id: ONBOARDING_REMINDER_ID,
  labelKey: "settings.appearance.reminders.onboarding",
  async isDismissed() {
    await onboardingStore.refreshState();
    return get(onboardingStore).started_at !== null;
  },
  async reset() {
    await resetOnboarding();
    // Reload the now-empty state so derived stores (badges, etc.) catch up,
    // then open the conversation at once, no restart required.
    await onboardingStore.refreshState();
    onboardingModalOpen.set(true);
  },
};

const nextStepSuggestions: Reminder = {
  id: NEXT_STEPS_REMINDER_ID,
  labelKey: "settings.appearance.reminders.next_steps",
  async isDismissed() {
    return nextSteps.hasDismissed();
  },
  async reset() {
    nextSteps.clearDismissed();
  },
};

export const REMINDERS: readonly Reminder[] = [onboarding, nextStepSuggestions];

/** Clear one reminder by id. Unknown ids are a programming error. */
export async function resetReminder(
  id: string,
  registry: readonly Reminder[] = REMINDERS,
): Promise<void> {
  const reminder = registry.find((r) => r.id === id);
  if (!reminder) throw new Error(`unknown reminder: ${id}`);
  await reminder.reset();
}

/** "Reset all": clear every reminder of the registry, in order. */
export async function resetAllReminders(
  registry: readonly Reminder[] = REMINDERS,
): Promise<void> {
  for (const reminder of registry) {
    await reminder.reset();
  }
}

/** The dismissed state of every reminder, by id. A failed read counts as not dismissed. */
export async function readReminderStates(
  registry: readonly Reminder[] = REMINDERS,
): Promise<Record<string, boolean>> {
  const entries = await Promise.all(
    registry.map(async (r) => {
      try {
        return [r.id, await r.isDismissed()] as const;
      } catch {
        return [r.id, false] as const;
      }
    }),
  );
  return Object.fromEntries(entries);
}
