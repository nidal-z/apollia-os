/**
 * Option list for the whisper model picker in the dictation settings.
 *
 * A `<select>` handed a value that no option carries selects nothing at all:
 * Svelte's binding falls through to `select.selectedIndex = -1`, so the field
 * renders blank. That is what made the model picker look empty while the status
 * row next to it, fed by the running engine instead of by the directory scan,
 * displayed the loaded model correctly.
 *
 * These helpers guarantee the configured model always has an option, whatever
 * the scan returned.
 */
import type { SttModelInfo } from "$lib/types";

/** Directory the desktop scans for whisper models. */
const MODELS_DIR = "~/.apollia/models";

/** One entry of the model picker. */
export interface ModelOption {
  value: string;
  label: string;
}

/** Last path segment, for both POSIX and Windows separators. */
export function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? "";
}

/** Canonical option value for a model file name. */
export function modelValue(name: string): string {
  return `${MODELS_DIR}/${name}`;
}

/**
 * Option value matching `configuredPath`.
 *
 * Normalises to the `~`-prefixed form so a model stored as an absolute path
 * (which is what the onboarding writes) still matches its option.
 */
export function selectedModelValue(configuredPath: string): string {
  const name = fileName(configuredPath);
  return name ? modelValue(name) : "";
}

/**
 * Builds the picker options, prepending the configured model when the scan did
 * not return it.
 *
 * A configured model can be missing from the scan because it lives outside
 * `~/.apollia/models`, or because it carries an extension the scan does not
 * know. Listing it anyway keeps the field truthful instead of blank.
 */
export function buildModelOptions(
  scanned: readonly SttModelInfo[],
  configuredPath: string,
): ModelOption[] {
  const options = scanned.map((model) => ({
    value: modelValue(model.name),
    label: `${model.name} (${model.size_mb.toFixed(0)} Mo${
      model.language ? ` · ${model.language}` : ""
    })`,
  }));

  const currentName = fileName(configuredPath);
  if (currentName && !scanned.some((model) => model.name === currentName)) {
    options.unshift({ value: modelValue(currentName), label: currentName });
  }

  return options;
}
