/**
 * Preset values for the throttle Select used in both create/edit dialogs.
 *
 * `null` is the sentinel for "Custom..." — when selected, the dialog shows
 * a NumberInput so the user can type any value up to MAX_MIN_INTERVAL_SECONDS.
 */
export const THROTTLE_PRESETS: ReadonlyArray<{ value: number; key: string }> = [
  { value: 0, key: "notifications.throttle_options.none" },
  { value: 60, key: "notifications.throttle_options.minute" },
  { value: 300, key: "notifications.throttle_options.five_min" },
  { value: 3600, key: "notifications.throttle_options.hour" },
];

/** Backend cap from `apollia_notifications::validation::MAX_MIN_INTERVAL_SECONDS`. */
export const MAX_MIN_INTERVAL_SECONDS = 86_400;

/**
 * Returns true if `value` matches one of the presets exactly.
 * Used to decide whether to show the dialog in "preset" or "custom" mode.
 */
export function isPreset(value: number): boolean {
  return THROTTLE_PRESETS.some((p) => p.value === value);
}
