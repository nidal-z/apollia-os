/**
 * The decisions of onboarding step 3, apart from the markup that renders them.
 *
 * Four rules live here: which regions of each section render, how a fresh voice
 * scan reconciles with a choice already on screen, and the whole lifecycle of
 * wiring a language engine. They were in the `<script module>` block of
 * `OnboardingAiSetup.svelte`, which three components now share; a plain module
 * keeps that sharing free of a cycle.
 */

/**
 * Which regions of the AI-setup step render, given what the scan found and
 * what the operator has already done.
 *
 * Both sections used to decide their branches inline in the template, and the
 * means of adding an engine lived inside the "nothing found" branch: one
 * `.gguf` anywhere on disk, or a first successful import, closed the door
 * that had just been used. The voice section had no branch at all for
 * "models present, dictation off", so it rendered an empty section. The
 * branches are pure data, so they are decided here and stay node-testable.
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

/** Render state of the speech-recognition section. */
export interface SttSectionView {
  /** The "no voice model found" hint. */
  showEmptyHint: boolean;
  /** The list of Whisper models the scan found on disk. */
  showDetectedList: boolean;
  /** Import a voice model from disk. */
  showAddMean: boolean;
  /** The curated Whisper models offered for download. */
  showCuratedList: boolean;
  /** Hotkey capture, microphone picker and live test. */
  showHotkeyBlock: boolean;
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

/**
 * Decide the speech-recognition regions.
 *
 * The list and the import button do not depend on the dictation toggle: the
 * toggle says whether dictation runs, not whether the section exists. Only
 * the hotkey and live-test block, which drives a running dictation, keeps the
 * toggle as a condition.
 */
export function sttSectionView(
  detectedCount: number,
  dictationEnabled: boolean,
): SttSectionView {
  return {
    showEmptyHint: detectedCount === 0,
    showDetectedList: detectedCount > 0,
    showAddMean: true,
    showCuratedList: detectedCount === 0,
    showHotkeyBlock: detectedCount > 0 && dictationEnabled,
  };
}

/** What a voice scan reports about one model, reduced to what a choice needs. */
export interface ScannedWhisperModel {
  /** Absolute path of the model file, its identity from one scan to the next. */
  path: string;
  /** Whether the scan marks this model as the one to use by default. */
  recommended: boolean;
}

/** The voice selection a scan leaves behind, and where it leaves the toggle. */
export interface WhisperScanChoice {
  /** Path of the model that stays selected, `null` when the scan found none. */
  selectedPath: string | null;
  /** Position of the dictation toggle once the scan has been applied. */
  dictationEnabled: boolean;
}

/**
 * Reconcile a fresh voice scan with the choice already on screen.
 *
 * A scan used to overwrite both the selected model and the dictation toggle
 * with the first result and its `recommended` flag, so any re-scan silently
 * undid what the operator had just picked. A choice that is still on disk is
 * therefore kept, toggle included.
 *
 * The first scan of a session has no choice to keep, and a chosen model that
 * the scan no longer reports is gone from disk: both fall back on the first
 * result, which is what makes the section usable without a click.
 */
export function reconcileWhisperScan(
  scanned: readonly ScannedWhisperModel[],
  currentPath: string | null,
  dictationEnabled: boolean,
): WhisperScanChoice {
  if (scanned.length === 0) {
    return { selectedPath: null, dictationEnabled: false };
  }
  if (currentPath !== null && scanned.some((model) => model.path === currentPath)) {
    return { selectedPath: currentPath, dictationEnabled };
  }
  const fallback = scanned[0];
  return { selectedPath: fallback.path, dictationEnabled: fallback.recommended };
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
