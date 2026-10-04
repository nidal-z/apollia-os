import { describe, expect, it } from "vitest";
import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";
import { embeddingsLabel } from "./embeddingsLabel";

const installed = { model: "multilingual-e5-small", dimensions: 384 };
const absent = { model: null, dimensions: null };

describe("embeddingsLabel", () => {
  it("names the model actually present for an episode and a fact", () => {
    // GIVEN an embeddings model installed
    // WHEN an episode and a fact are labelled
    const episode = embeddingsLabel("episodic", installed);
    const fact = embeddingsLabel("semantic", installed);

    // THEN both name that model and its dimension
    const expected = {
      key: "memory.embeddings.vector",
      values: { model: "multilingual-e5-small", dimensions: 384 },
    };
    expect(episode).toEqual(expected);
    expect(fact).toEqual(expected);
  });

  it("says keywords when no model is present", () => {
    // GIVEN no embeddings model, or none read yet
    // WHEN entries are labelled
    const noModel = embeddingsLabel("semantic", absent);
    const unread = embeddingsLabel("episodic", null);

    // THEN they are searched by keywords
    expect(noModel).toEqual({ key: "memory.embeddings.keywords" });
    expect(unread).toEqual({ key: "memory.embeddings.keywords" });
  });

  it("never gives a procedure a vector", () => {
    // GIVEN a model installed
    // WHEN a procedure is labelled
    const label = embeddingsLabel("procedural", installed);

    // THEN it has no vector
    expect(label).toEqual({ key: "memory.embeddings.none" });
  });

  it("no longer names a model the code does not have", () => {
    // GIVEN the catalogues of both languages
    // WHEN the vector label is read
    const labels = [en.memory.embeddings.vector, fr.memory.embeddings.vector];

    // THEN it carries the model in force, never a fixed name
    for (const label of labels) {
      expect(label).not.toContain("nomic");
      expect(label).toContain("{model}");
      expect(label).toContain("{dimensions}");
    }
    expect(en.memory.embeddings.keywords).toBe("keywords");
    expect(fr.memory.embeddings.keywords).not.toBe("");
    expect(fr.memory.embeddings.keywords).not.toBe(en.memory.embeddings.keywords);
  });
});
