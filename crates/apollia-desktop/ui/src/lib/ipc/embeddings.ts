import { invoke } from "@tauri-apps/api/core";

/** One embeddings model of the registry, pinned to one GGUF file. */
export interface EmbeddingModel {
  id: string;
  label: string;
  publisher: string;
  license: string;
  /** HuggingFace repository carrying the file, `owner/name`. */
  repo: string;
  /** The GGUF file within the repository. */
  file: string;
  quantization: string;
  size_bytes: number;
  sha256: string;
  dimensions: number;
  context_length: number;
}

/** Why the recommended entry was chosen, footprints in gibibytes. */
export type EmbeddingReason =
  | { kind: "fits_beside"; language_gb: number; embeddings_gb: number; capacity_gb: number }
  | {
      kind: "no_room_beside";
      language_gb: number;
      larger: string;
      larger_gb: number;
      capacity_gb: number;
    };

/** The embeddings models and the one for this machine, as `GET /api/v1/llm/embeddings` answers. */
export interface EmbeddingsRecommendation {
  models: EmbeddingModel[];
  /** Identifier of the recommended entry, `null` when the registry has none. */
  recommended: string | null;
  reason: EmbeddingReason | null;
}

/** The default embeddings model, as `PUT /api/v1/llm/embeddings/default` answers. */
export interface EmbeddingDefault {
  model: string | null;
  installed: boolean;
  path: string | null;
}

/**
 * The embeddings models of the registry and the one recommended for this
 * machine. Answered without the network. `languageModelBytes` is the size of
 * the language model it has to fit beside; absent, the largest installed one.
 */
export function getEmbeddingsRecommendation(
  languageModelBytes: number | null,
): Promise<EmbeddingsRecommendation> {
  return invoke<EmbeddingsRecommendation>("get_embeddings_recommendation", {
    languageModelBytes,
  });
}

/** Makes `model` the embeddings model search by meaning uses. */
/** What the embeddings model answered to its trial. */
export interface EmbeddingsTest {
  /** Identifier of the model that answered. */
  model: string;
  /** Number of values of the vector it returned. */
  dimensions: number;
  /** Time from the request to the vector, loading included. */
  elapsed_ms: number;
}

/**
 * Tries the embeddings model search uses on one sentence: it is loaded and
 * answers a vector. Rejects with the runtime's reason when the machine is not
 * ready (no model chosen, file absent, no local engine) or the model failed.
 */
export function testEmbeddingsModel(): Promise<EmbeddingsTest> {
  return invoke<EmbeddingsTest>("test_embeddings_model");
}

export function setDefaultEmbeddingsModel(model: string): Promise<EmbeddingDefault> {
  return invoke<EmbeddingDefault>("set_default_embeddings_model", { model });
}
