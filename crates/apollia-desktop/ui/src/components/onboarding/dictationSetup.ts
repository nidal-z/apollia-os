/**
 * The dictation model of the first launch, set up like the model of search:
 * the one recommended for this machine is offered, downloaded or imported,
 * then put in use.
 *
 * The setup itself is the one of the embeddings model
 * (`createEmbeddingsSetup`): a model has a repository, a file and a size
 * whatever it is for. What differs lives here, as plain functions that the
 * tests call without the application: which model is already in use, where a
 * dictation file is fetched, and what "put in use" means for dictation (the
 * dictation settings name its file and dictation is switched on).
 */
import type { EmbeddingDefault, EmbeddingModel, EmbeddingsRecommendation } from "$lib/ipc/embeddings";
import type { DictationModel, DictationModels, VariantDownloadRequest } from "$lib/ipc/modelHub";
import type { MemoryEmbeddings, SttConfigView, SttModelInfo } from "$lib/types";

/** The extension of a dictation file. */
export const DICTATION_EXTENSIONS = ["bin"];

function baseName(path: string): string {
  return path.split(/[\\/]/).pop() ?? "";
}

/**
 * The dictation models as the setup reads them. A dictation model has no
 * vector size and no context: both are zero, and nothing of the setup reads
 * them.
 */
export function asSetupModels(answer: DictationModels): EmbeddingsRecommendation {
  const models: EmbeddingModel[] = answer.models.map((model: DictationModel) => ({
    id: model.id,
    label: model.label,
    publisher: model.publisher,
    license: model.license,
    repo: model.repo,
    file: model.file,
    quantization: model.quantization,
    size_bytes: model.size_bytes,
    sha256: model.sha256,
    dimensions: 0,
    context_length: 0,
  }));
  return { models, recommended: answer.recommended, reason: null };
}

/**
 * The dictation model in use: the entry whose file the dictation settings
 * name, when dictation is on and that file is installed. Nothing otherwise,
 * so the step offers the recommended model.
 */
export function modelInUse(
  config: SttConfigView,
  installed: readonly SttModelInfo[],
  models: readonly DictationModel[],
): MemoryEmbeddings {
  const none = { model: null, dimensions: null };
  if (!config.enabled || !config.model_path) return none;
  if (!installed.some((entry) => entry.path === config.model_path)) return none;
  const file = baseName(config.model_path).toLowerCase();
  const entry = models.find((model) => model.file.toLowerCase() === file);
  return entry ? { model: entry.id, dimensions: null } : none;
}

/** Where the file of a dictation model is fetched: one file, at a pinned address. */
export function dictationDownload(request: VariantDownloadRequest): {
  url: string;
  filename: string;
  repo_id: string;
} {
  const file = request.selector.kind === "gguf" ? request.selector.file : "";
  return {
    url: `https://huggingface.co/${request.repo_id}/resolve/main/${file}`,
    filename: file,
    repo_id: request.repo_id,
  };
}

/** What putting a dictation model in use reaches. */
export interface DictationUseDeps {
  models: () => Promise<DictationModels>;
  installed: () => Promise<SttModelInfo[]>;
  config: () => Promise<SttConfigView>;
  save: (config: SttConfigView) => Promise<void>;
  reload: () => Promise<void>;
}

/**
 * Puts the dictation model `id` in use: the dictation settings name its
 * installed file, dictation is switched on, and the engine reloads. Rejects
 * when the model is not of the registry or its file is not on this machine.
 */
export async function useDictationModel(id: string, deps: DictationUseDeps): Promise<EmbeddingDefault> {
  const entry = (await deps.models()).models.find((model) => model.id === id);
  if (!entry) throw new Error(`${id} is not a dictation model of the registry`);
  const file = entry.file.toLowerCase();
  const found = (await deps.installed()).find((model) => baseName(model.path).toLowerCase() === file);
  if (!found) throw new Error(`the file ${entry.file} is not installed`);
  const config = await deps.config();
  await deps.save({ ...config, enabled: true, model_path: found.path });
  await deps.reload();
  return { model: id, installed: true, path: found.path };
}
