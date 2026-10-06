/**
 * The decisions of the AI step of the first launch, apart from the markup that
 * renders them.
 *
 * Three rules live here: which regions of the language-engine section render,
 * the whole lifecycle of wiring an engine, and when the step may be left. A
 * plain module keeps them shared between the step and its section without a
 * cycle, and within reach of a test.
 */

/**
 * Which regions of the language-engine section render, given what the scan
 * found and what the operator has already done.
 *
 * The section used to decide its branches inline in the template, and the
 * means of adding an engine lived inside the "nothing found" branch: one
 * `.gguf` anywhere on disk, or a first successful import, closed the door
 * that had just been used. The branches are pure data, so they are decided
 * here and stay node-testable.
 */

/** Render state of the language-engine section. */
export interface LlmSectionView {
  /** The "drop a model in these folders" hint. */
  showEmptyHint: boolean;
  /** The list of GGUF files the scan found on disk. */
  showDetectedList: boolean;
  /** The row naming the engine wired up during this session. */
  showSuccessRow: boolean;
  /** Import from disk, curated catalogue, HuggingFace search. */
  showAddMeans: boolean;
}

/**
 * Decide the language-engine regions.
 *
 * `showAddMeans` is unconditional: an operator who already owns one engine is
 * the operator most likely to want a second one, and onboarding is the only
 * guided moment where the three ways of adding one are shown together.
 */
export function llmSectionView(
  detectedCount: number,
  configuredInSession: boolean,
): LlmSectionView {
  return {
    showEmptyHint: detectedCount === 0 && !configuredInSession,
    showDetectedList: detectedCount > 0,
    showSuccessRow: configuredInSession,
    showAddMeans: true,
  };
}

/** The engine-configuration state of the step, as the template reads it. */
export interface LlmConfigurationState {
  /** Path of the engine the step names, `null` while none is wired. */
  selectedPath: string | null;
  /** True only while a configuration is in flight. */
  configuring: boolean;
  /** True once an engine has been wired during this session. */
  configured: boolean;
  /** Message of the configuration that failed, `null` otherwise. */
  error: string | null;
}

/**
 * Run one engine configuration, from the click to the settled state.
 *
 * `wire` performs the effect, `publish` receives the in-flight state and then
 * the settled one. The lock comes back down whichever way the run ends. A run
 * that succeeded used to leave it raised for the rest of the session, which
 * disabled every detected row and turned the confirmation into a dead end:
 * having wired one engine is exactly when an operator wants another.
 *
 * A run that failed restores the engine wired before it, so the step keeps
 * naming the one actually in use rather than the one that just refused.
 */
export async function runLlmConfiguration(
  current: LlmConfigurationState,
  path: string,
  wire: (path: string) => Promise<void>,
  publish: (next: LlmConfigurationState) => void,
): Promise<void> {
  publish({
    selectedPath: path,
    configuring: true,
    configured: current.configured,
    error: null,
  });
  try {
    await wire(path);
    publish({ selectedPath: path, configuring: false, configured: true, error: null });
  } catch (err: unknown) {
    publish({
      selectedPath: current.selectedPath,
      configuring: false,
      configured: current.configured,
      error: err instanceof Error ? err.message : String(err),
    });
  }
}

/**
 * Extension of the language data optical character recognition reads with.
 * The import from disk offers it beside the model files: on a machine without
 * the network it is the only way these files reach the application.
 */
export const LANGUAGE_DATA_EXTENSION = "traineddata";

/** Whether the file at `path` is language data of recognition, not a model. */
export function isLanguageData(path: string): boolean {
  return path.toLowerCase().endsWith(`.${LANGUAGE_DATA_EXTENSION}`);
}

/** Where the AI of the agents runs: on this computer, or on a server. */
export type AiSource = "local" | "server";

/** What the AI step knows when "Continue" is asked for. */
export interface AiStepProgress {
  /** The machine is still being probed: the section is not on screen yet. */
  probing: boolean;
  /** An engine was wired during this session. */
  engineWired: boolean;
  /** Engines the runtime already holds. */
  backendCount: number;
  /** A model of a server was chosen in the step and is the default model. */
  serverChosen: boolean;
  /** A language model is being downloaded, or has landed and is being wired. */
  downloadInFlight: boolean;
}

/**
 * Whether the AI step may be left. It is the only step of the first launch
 * that cannot be passed: without a model or a server, no agent answers.
 *
 * A download that was launched is enough. The steps that follow are done
 * while the file arrives, and the one that needs the model waits for it.
 */
export function canContinueAiStep(progress: AiStepProgress): boolean {
  if (progress.probing) return false;
  return (
    progress.engineWired ||
    progress.backendCount > 0 ||
    progress.serverChosen ||
    progress.downloadInFlight
  );
}
