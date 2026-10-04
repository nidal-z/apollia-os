/**
 * What the Memory screen says beside an entry about how it is searched: by
 * meaning with the embeddings model actually present, or by its words alone.
 *
 * Episodes and facts get a vector once a model is installed; procedures never
 * do. Without a model, every entry is searched by keywords.
 */

import type { MemoryEmbeddings } from "$lib/types";

export type EmbeddingsLabel =
  | { key: "memory.embeddings.vector"; values: { model: string; dimensions: number } }
  | { key: "memory.embeddings.keywords" }
  | { key: "memory.embeddings.none" };

/** The label of an entry of type `entryType`, given the model present. */
export function embeddingsLabel(
  entryType: string,
  embeddings: MemoryEmbeddings | null,
): EmbeddingsLabel {
  if (entryType !== "episodic" && entryType !== "semantic") {
    return { key: "memory.embeddings.none" };
  }
  if (embeddings?.model && embeddings.dimensions) {
    return {
      key: "memory.embeddings.vector",
      values: { model: embeddings.model, dimensions: embeddings.dimensions },
    };
  }
  return { key: "memory.embeddings.keywords" };
}
