import { describe, it, expect } from "vitest";
import enCatalogue from "$lib/i18n/en.json";
import frCatalogue from "$lib/i18n/fr.json";
import { documentLabel, documentsText, messageDocuments, typedText } from "./messageDocuments";

function lookup(catalogue: unknown, key: string): unknown {
  return key
    .split(".")
    .reduce<unknown>(
      (node, part) =>
        node && typeof node === "object" ? (node as Record<string, unknown>)[part] : undefined,
      catalogue,
    );
}

describe("messageDocuments", () => {
  it("lists the documents a reloaded message carries, and those of a message just sent", () => {
    // GIVEN the metadata the runtime wrote, and the metadata of a message just sent
    const stored = {
      documents: [
        { file: "ab-f.pdf", name: "f.pdf", size: 10, unit: "page", parts: 10, characters: 900, inline: true },
        "noise",
      ],
    };
    const sent = { documents: [{ file: "", name: "f.pdf", size: 10 }] };

    // WHEN each is read
    // THEN the documents come back by name, the entry that is not one dropped
    expect(messageDocuments(stored).map((d) => d.name)).toEqual(["f.pdf"]);
    expect(messageDocuments(sent).map((d) => d.name)).toEqual(["f.pdf"]);
    expect(messageDocuments(null)).toEqual([]);
    expect(messageDocuments({ images: [] })).toEqual([]);
  });
});

describe("typedText", () => {
  it("shows what the user typed, without the text of the document", () => {
    // GIVEN a content where the text of a document follows a question with an accent and an emoji
    const question = "Quel est le montant de la facture Durand ? 🧾";
    const block = '\n\n<attachment name="f.pdf" size="10" pages="1">\nTotal 4 200 €\n</attachment>';
    const metadata = { documents_start: question.length };

    // WHEN the content is split
    // THEN the bubble shows the question alone, and the rest is the document
    expect(typedText(question + block, metadata)).toBe(question);
    expect(documentsText(question + block, metadata)).toBe(block);
  });

  it("shows the whole content of a message without documents", () => {
    // GIVEN a content and no document metadata, or one that is not a number
    // WHEN it is split
    // THEN everything is typed text
    expect(typedText("bonjour", null)).toBe("bonjour");
    expect(documentsText("bonjour", null)).toBe("");
    expect(typedText("bonjour", { documents_start: "3" })).toBe("bonjour");
    expect(typedText("bonjour", { documents_start: 99 })).toBe("bonjour");
  });
});

describe("documentLabel", () => {
  it("says what the document holds and that its text was read", () => {
    // GIVEN a PDF of ten pages whose text is in the message
    const document = { name: "f.pdf", size: 1, unit: "page", parts: 10, characters: 900, inline: true };

    // WHEN it is described
    // THEN it counts its pages and says the text was read
    expect(documentLabel(document)).toEqual({
      key: "chat.documents.read_pages",
      values: { count: 10 },
    });
    expect(lookup(frCatalogue, "chat.documents.read_pages")).toContain("texte lu");
  });

  it("says a long document is read on demand, and a scanned one has no text", () => {
    // GIVEN a workbook past the inline ceiling, and a scanned PDF
    const long = { name: "g.xlsx", size: 1, unit: "sheet", parts: 3, characters: 250000, inline: false };
    const scanned = { name: "s.pdf", size: 1, unit: "page", parts: 2, characters: 0, inline: true };

    // WHEN they are described
    // THEN the first is read on demand and the second holds no text
    expect(documentLabel(long)).toEqual({
      key: "chat.documents.on_demand_sheets",
      values: { count: 3 },
    });
    expect(documentLabel(scanned)).toEqual({ key: "chat.documents.no_text" });
  });

  it("has no count for a Word document, and no label before the runtime read it", () => {
    // GIVEN a Word document read, and a document just sent
    const word = { name: "r.docx", size: 1, unit: "document", parts: 1, characters: 40, inline: true };
    const sent = { name: "r.docx", size: 1 };

    // WHEN they are described
    // THEN the first has a state without a count, the second nothing yet
    expect(documentLabel(word)).toEqual({ key: "chat.documents.read_whole" });
    expect(documentLabel(sent)).toBeNull();
  });

  it("has a sentence in both catalogues for every label", () => {
    // GIVEN every document a label can describe
    const units = ["page", "slide", "sheet", "document"];
    const keys = new Set<string>(["chat.documents.no_text"]);
    for (const unit of units) {
      for (const inline of [true, false]) {
        const label = documentLabel({ name: "x", size: 1, unit, parts: 2, characters: 5, inline });
        if (label) keys.add(label.key);
      }
    }

    // WHEN each catalogue is read
    // THEN the nine keys resolve to a sentence in French and in English
    expect(keys.size).toBe(9);
    for (const key of keys) {
      expect(typeof lookup(enCatalogue, key), key).toBe("string");
      expect(typeof lookup(frCatalogue, key), key).toBe("string");
    }
  });
});
