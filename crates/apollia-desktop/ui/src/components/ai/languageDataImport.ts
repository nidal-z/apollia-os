/**
 * The import of a language data file of recognition from the AI screen: the
 * same file the welcome step takes, offered after it, for a computer that
 * cannot download one.
 */

import { LANGUAGE_DATA_EXTENSION } from "../onboarding/aiSetupRules";

/** What an import ended on. */
export type LanguageDataImport =
  | { kind: "cancelled" }
  | { kind: "imported"; file: string }
  | { kind: "unknown" }
  | { kind: "failed"; error: unknown };

/** What the import needs from the window and from the runtime. */
export interface LanguageDataImportDeps {
  /** Opens the file picker filtered on language data; `null` when cancelled. */
  pick: (extensions: string[]) => Promise<string | null>;
  /** The `import_model_file` command. */
  importModelFile: (sourcePath: string, allowedExtensions: string[]) => Promise<string>;
}

/**
 * Whether `err` is the refusal of a file that is no language data this build
 * reads. The runtime refuses a digest the registry does not pin with a message
 * saying it "is not language data" (`OcrDataError::UnknownDigest`), and a file
 * of another type as an "unsupported file type".
 */
export function isUnknownLanguageData(err: unknown): boolean {
  const message = err instanceof Error ? err.message : String(err);
  return /is not language data|unsupported file type/i.test(message);
}

/** The toast an import ends on: its message key, its values, its tone. */
export interface LanguageDataToast {
  key: string;
  values?: Record<string, string>;
  tone: "success" | "error";
}

/**
 * The toast of `outcome`; `null` for a cancelled pick, and for a failure the
 * caller reports as any other error. The success says what the welcome step
 * says.
 */
export function languageDataToast(outcome: LanguageDataImport): LanguageDataToast | null {
  switch (outcome.kind) {
    case "imported":
      return {
        key: "onboarding.ai_setup.language_data_imported",
        values: { file: outcome.file },
        tone: "success",
      };
    case "unknown":
      return { key: "ai.toast.language_data_unknown", tone: "error" };
    default:
      return null;
  }
}

/** Asks for a language data file, then brings it into the models folder. */
export async function importLanguageData(deps: LanguageDataImportDeps): Promise<LanguageDataImport> {
  const extensions = [LANGUAGE_DATA_EXTENSION];
  try {
    const path = await deps.pick(extensions);
    if (!path) return { kind: "cancelled" };
    const dest = await deps.importModelFile(path, extensions);
    return { kind: "imported", file: dest.split(/[\\/]/).pop() ?? dest };
  } catch (error: unknown) {
    return isUnknownLanguageData(error) ? { kind: "unknown" } : { kind: "failed", error };
  }
}
