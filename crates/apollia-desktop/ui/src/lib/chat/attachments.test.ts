import { describe, it, expect } from "vitest";
import enCatalogue from "$lib/i18n/en.json";
import frCatalogue from "$lib/i18n/fr.json";
import {
  type AttachmentCandidate,
  type AttachmentRefusal,
  type PendingAttachment,
  classifyKind,
  composeUserPayload,
  documentPayloads,
  imagePayloads,
  intakeAttachment,
  refusalMessageKey,
  sentAttachmentsMetadata,
  MAX_DOCUMENTS_PER_MESSAGE,
  MAX_DOCUMENT_BYTES,
  MAX_IMAGES_PER_MESSAGE,
  MAX_IMAGE_BYTES,
} from "./attachments";

function candidate(over: Partial<AttachmentCandidate>): AttachmentCandidate {
  return {
    id: "att-1",
    name: "notes.md",
    mime: "text/markdown",
    size: 1024,
    kind: "text",
    ...over,
  };
}

function base64(text: string): string {
  return Buffer.from(text, "utf-8").toString("base64");
}

/** A real PDF header: ASCII magic, then the four high bytes producers emit. */
function pdfHeaderBase64(): string {
  return Buffer.from([
    0x25, 0x50, 0x44, 0x46, 0x2d, 0x31, 0x2e, 0x37, 0x0a, 0x25, 0xe2, 0xe3, 0xcf, 0xd3, 0x0a, 0x31,
    0x20, 0x30, 0x20, 0x6f, 0x62, 0x6a, 0x0a,
  ]).toString("base64");
}

function lookup(catalogue: unknown, key: string): unknown {
  return key
    .split(".")
    .reduce<unknown>(
      (node, part) =>
        node && typeof node === "object" ? (node as Record<string, unknown>)[part] : undefined,
      catalogue,
    );
}

describe("intakeAttachment, every shape a picked file can take", () => {
  it("accepts a small readable text file as an inline payload", () => {
    // GIVEN a text file the composer managed to read
    const input = candidate({ base64: base64("hello") });

    // WHEN it goes through intake
    const intake = intakeAttachment(input);

    // THEN it is queued, and it will travel as inline text
    expect(intake.accepted).toBe(true);
    if (!intake.accepted) return;
    expect(intake.attachment.form).toBe("inline_text");
  });

  it("accepts a dropped file as a path reference", () => {
    // GIVEN a file dropped from the desktop, past the inline ceiling
    const input = candidate({ size: 4_000_000, absolutePath: "/Users/x/big.csv" });

    // WHEN it goes through intake
    const intake = intakeAttachment(input);

    // THEN it is queued, and it will travel as a path
    expect(intake.accepted).toBe(true);
    if (!intake.accepted) return;
    expect(intake.attachment.form).toBe("path_reference");
  });

  it("keeps a dropped small text file inline rather than behind an approval", () => {
    // GIVEN a small text file dropped from the desktop, so it carries both
    const input = candidate({ base64: base64("hello"), absolutePath: "/Users/x/notes.md" });

    // WHEN it goes through intake
    const intake = intakeAttachment(input);

    // THEN the inline payload wins, the path is only a fallback
    expect(intake.accepted).toBe(true);
    if (!intake.accepted) return;
    expect(intake.attachment.form).toBe("inline_text");
  });

  it("falls back to the path when the inline read of a dropped file failed", () => {
    // GIVEN a dropped file whose inline read threw but whose path is known
    const input = candidate({ readFailed: true, absolutePath: "/Users/x/notes.md" });

    // WHEN it goes through intake
    const intake = intakeAttachment(input);

    // THEN it still travels, as a path, instead of being dropped silently
    expect(intake.accepted).toBe(true);
    if (!intake.accepted) return;
    expect(intake.attachment.form).toBe("path_reference");
  });

  it("refuses an image whose only payload would be inline base64", () => {
    // GIVEN a small image, read inline, with no path behind it
    const input = candidate({
      name: "shot.png",
      mime: "image/png",
      kind: "image",
      size: 400 * 1024,
      base64: base64("PNGBYTES"),
    });

    // WHEN it goes through intake
    const intake = intakeAttachment(input);

    // THEN it is refused: no model known to read images, nothing reads its bytes
    expect(intake.accepted).toBe(false);
    if (intake.accepted) return;
    expect(intake.reason).toBe("image_not_supported");
  });

  it("accepts an image that carries a path", () => {
    // GIVEN the same image, dropped instead of picked
    const input = candidate({
      name: "shot.png",
      mime: "image/png",
      kind: "image",
      size: 400 * 1024,
      absolutePath: "/Users/x/shot.png",
    });

    // WHEN it goes through intake
    const intake = intakeAttachment(input);

    // THEN it is queued as a path, the one form the tools can act on
    expect(intake.accepted).toBe(true);
    if (!intake.accepted) return;
    expect(intake.attachment.form).toBe("path_reference");
  });

  it("refuses a large file picked through the paperclip, which carries no path", () => {
    // GIVEN a file past the inline ceiling, picked rather than dropped
    const input = candidate({ name: "dump.log", size: 4_000_000 });

    // WHEN it goes through intake
    const intake = intakeAttachment(input);

    // THEN it is refused rather than queued as an empty tag
    expect(intake.accepted).toBe(false);
    if (intake.accepted) return;
    expect(intake.reason).toBe("too_large_without_path");
  });

  it("refuses a small archive picked through the paperclip, a format that is not read", () => {
    // GIVEN a 300 kB archive, small enough that the composer read it inline
    const input = candidate({
      name: "rapport.zip",
      mime: "application/zip",
      kind: "other",
      size: 307_200,
      base64: pdfHeaderBase64(),
    });

    // WHEN it goes through intake
    const intake = intakeAttachment(input);

    // THEN it is refused rather than inlined as replacement characters
    expect(intake.accepted).toBe(false);
    if (intake.accepted) return;
    expect(intake.reason).toBe("binary_not_text");
  });

  it("falls back to the path for a dropped file whose bytes are not text", () => {
    // GIVEN the same archive dropped from the desktop, so it also carries a path
    const input = candidate({
      name: "rapport.zip",
      mime: "application/zip",
      kind: "other",
      size: 307_200,
      base64: pdfHeaderBase64(),
      absolutePath: "/Users/x/rapport.zip",
    });

    // WHEN it goes through intake
    const intake = intakeAttachment(input);

    // THEN it still travels, under the form the filesystem tools can read
    expect(intake.accepted).toBe(true);
    if (!intake.accepted) return;
    expect(intake.attachment.form).toBe("path_reference");
  });

  it("keeps accepting accented text inline, which the byte check must not catch", () => {
    // GIVEN a text file whose bytes are multi-byte UTF-8 rather than ASCII
    const input = candidate({ name: "notes.md", base64: base64("éàü, 日本語, emoji 🙂") });

    // WHEN it goes through intake
    const intake = intakeAttachment(input);

    // THEN it is still queued inline: the guard rejects binary, not non-ASCII
    expect(intake.accepted).toBe(true);
    if (!intake.accepted) return;
    expect(intake.attachment.form).toBe("inline_text");
  });

  it("refuses a file whose read threw", () => {
    // GIVEN a small file whose inline read failed
    const input = candidate({ readFailed: true });

    // WHEN it goes through intake
    const intake = intakeAttachment(input);

    // THEN the failure reaches the user instead of being swallowed
    expect(intake.accepted).toBe(false);
    if (intake.accepted) return;
    expect(intake.reason).toBe("read_failed");
  });
});

describe("composeUserPayload, every queued attachment carries content or a path", () => {
  it("inlines the decoded text of an inline attachment", () => {
    // GIVEN a queued inline text attachment
    const intake = intakeAttachment(candidate({ base64: base64("# Title\nbody") }));
    if (!intake.accepted) throw new Error("expected an accepted intake");

    // WHEN the message is composed
    const payload = composeUserPayload("look at this", [intake.attachment]);

    // THEN the decoded text rides inside the tag
    expect(payload).toContain("look at this");
    expect(payload).toContain("# Title\nbody");
  });

  it("references a path attachment by its absolute path", () => {
    // GIVEN a queued path attachment
    const intake = intakeAttachment(
      candidate({ name: "big.csv", size: 4_000_000, absolutePath: "/Users/x/big.csv" }),
    );
    if (!intake.accepted) throw new Error("expected an accepted intake");

    // WHEN the message is composed
    const payload = composeUserPayload("", [intake.attachment]);

    // THEN the path rides inside the tag
    expect(payload).toContain('path="/Users/x/big.csv"');
  });

  it("never emits a tag carrying neither content nor path", () => {
    // GIVEN every candidate shape the composer can build, refused ones included
    const shapes: AttachmentCandidate[] = [
      candidate({ base64: base64("inline") }),
      candidate({ size: 4_000_000, absolutePath: "/Users/x/big.csv" }),
      candidate({ name: "shot.png", mime: "image/png", kind: "image", base64: base64("IMG") }),
      candidate({ name: "dump.log", size: 4_000_000 }),
      candidate({ readFailed: true }),
      candidate({
        name: "rapport.pdf",
        mime: "application/pdf",
        kind: "other",
        size: 307_200,
        base64: pdfHeaderBase64(),
      }),
    ];

    // WHEN only the accepted ones reach the composition
    const queued = shapes
      .map((c) => intakeAttachment(c))
      .filter((r): r is Extract<typeof r, { accepted: true }> => r.accepted)
      .map((r) => r.attachment);
    const payload = composeUserPayload("hi", queued);

    // THEN no attachment tag is emitted without a payload or a path
    const tags = payload.match(/<attachment[^>]*>/g) ?? [];
    expect(tags.length).toBeGreaterThan(0);
    for (const tag of tags) {
      expect(tag.includes("path=") || tag.includes("mime=")).toBe(true);
    }
    expect(payload).not.toContain("IMG");
    // AND no inlined attachment carries the replacement character, which is
    // what a binary read as UTF-8 turns into.
    expect(payload).not.toContain("\uFFFD");
  });
});

describe("refusalMessageKey, every refusal has a sentence in both catalogues", () => {
  const reasons: AttachmentRefusal[] = [
    "read_failed",
    "image_not_supported",
    "image_limit",
    "image_too_large",
    "image_unreadable",
    "image_format",
    "too_large_without_path",
    "document_limit",
    "document_too_large",
    "binary_not_text",
  ];

  for (const reason of reasons) {
    it(`names a key present in en and fr for ${reason}`, () => {
      // GIVEN a refusal reason
      const key = refusalMessageKey(reason);

      // WHEN both catalogues are resolved at that key
      const en = lookup(enCatalogue, key);
      const fr = lookup(frCatalogue, key);

      // THEN each carries a non-empty sentence naming the file
      expect(typeof en).toBe("string");
      expect(typeof fr).toBe("string");
      expect(en as string).toContain("{name}");
      expect(fr as string).toContain("{name}");
    });
  }
});

describe("intakeAttachment, images and the model of the conversation", () => {
  const image = (over: Partial<AttachmentCandidate> = {}): AttachmentCandidate =>
    candidate({ name: "shot.png", mime: "image/png", kind: "image", base64: base64("IMG"), ...over });

  it("queues an image as its bytes when the model reads images", () => {
    // GIVEN a picked image, read, and a model with vision
    // WHEN it goes through intake
    const intake = intakeAttachment(image(), { vision: true, queuedImages: 0 });

    // THEN it is queued in the image form, its bytes kept
    expect(intake.accepted).toBe(true);
    if (!intake.accepted) return;
    expect(intake.attachment.form).toBe("image");
    expect(intake.attachment.base64).toBe(base64("IMG"));
  });

  it("keeps the path-or-refuse rule when the model does not read images", () => {
    // GIVEN the same image to a model without vision, then one dropped
    const picked = intakeAttachment(image(), { vision: false });
    const dropped = intakeAttachment(image({ absolutePath: "/tmp/shot.png" }), { vision: false });

    // WHEN intake decides
    // THEN the picked one is refused with the cause, the dropped one is a path
    expect(picked.accepted).toBe(false);
    if (!picked.accepted) expect(picked.reason).toBe("image_not_supported");
    expect(dropped.accepted).toBe(true);
    if (dropped.accepted) expect(dropped.attachment.form).toBe("path_reference");
  });

  it("refuses the ninth image of a message, and an image past the size ceiling", () => {
    // GIVEN eight images already queued, then an image of 10 MiB plus one byte
    const ninth = intakeAttachment(image(), {
      vision: true,
      queuedImages: MAX_IMAGES_PER_MESSAGE,
    });
    const heavy = intakeAttachment(
      image({ base64: Buffer.alloc(MAX_IMAGE_BYTES + 1).toString("base64") }),
      { vision: true, queuedImages: 0 },
    );

    // WHEN intake decides
    // THEN both are refused, each with its reason
    expect(ninth.accepted).toBe(false);
    if (!ninth.accepted) expect(ninth.reason).toBe("image_limit");
    expect(heavy.accepted).toBe(false);
    if (!heavy.accepted) expect(heavy.reason).toBe("image_too_large");
  });

  it("sends images beside the text, never inside it", () => {
    // GIVEN a message with a text file and two images queued
    const queued: PendingAttachment[] = [
      { id: "a", name: "a.png", mime: "image/png", size: 3, kind: "image", form: "image", base64: "AQID" },
      { id: "b", name: "b.jpg", mime: "image/jpeg", size: 2, kind: "image", form: "image", base64: "BAU=" },
    ];

    // WHEN the text and the image payloads are built
    const text = composeUserPayload("compare them", queued);
    const images = imagePayloads(queued);

    // THEN the text is unchanged and the images travel in order
    expect(text).toBe("compare them");
    expect(images).toEqual([
      { mime_type: "image/png", data: "AQID" },
      { mime_type: "image/jpeg", data: "BAU=" },
    ]);
  });
});

describe("documents the runtime reads", () => {
  function document(over: Partial<AttachmentCandidate> = {}): AttachmentCandidate {
    return candidate({
      name: "invoices.pdf",
      mime: "application/pdf",
      kind: "document",
      size: 307_200,
      base64: pdfHeaderBase64(),
      ...over,
    });
  }

  it("classifies every format that is read as a document, whatever its case", () => {
    // GIVEN one file name per format the runtime reads, and names it does not
    const read = ["a.pdf", "b.DOCX", "c.pptx", "d.xlsx", "e.xlsm", "f.xls", "g.ods"];
    const notRead = ["h.zip", "i.doc", "j.exe"];

    // WHEN each is classified
    // THEN the first are documents, the others are not, and a CSV stays text
    for (const name of read) expect(classifyKind("", name), name).toBe("document");
    for (const name of notRead) expect(classifyKind("", name), name).toBe("other");
    expect(classifyKind("", "k.csv")).toBe("text");
  });

  it("accepts a PDF picked through the paperclip, where it used to be refused", () => {
    // GIVEN a PDF picked through the paperclip: bytes, no path
    const input = document();

    // WHEN it goes through intake
    const intake = intakeAttachment(input);

    // THEN it is queued as a document, not refused as bytes that are not text
    expect(intake.accepted).toBe(true);
    if (!intake.accepted) return;
    expect(intake.attachment.form).toBe("document");
  });

  it("accepts a Word, a PowerPoint and an Excel file the same way", () => {
    for (const name of ["memo.docx", "deck.pptx", "sales.xlsx", "macro.xlsm", "old.xls", "sheet.ods"]) {
      // GIVEN a file of a format that is read, picked through the paperclip
      const input = document({ name });

      // WHEN it goes through intake
      const intake = intakeAttachment(input);

      // THEN it is queued as a document
      expect(intake.accepted, name).toBe(true);
      if (!intake.accepted) return;
      expect(intake.attachment.form, name).toBe("document");
    }
  });

  it("accepts a CSV file that is not UTF-8 as a document, and a UTF-8 one as text", () => {
    // GIVEN a CSV file saved in Windows-1252, and one in UTF-8
    const latin = candidate({
      name: "customers.csv",
      mime: "text/csv",
      base64: Buffer.from([0x6e, 0x6f, 0x6d, 0x3b, 0x42, 0x65, 0x73, 0x61, 0x6e, 0xe7, 0x6f, 0x6e]).toString("base64"),
    });
    const utf8 = candidate({ name: "customers.csv", mime: "text/csv", base64: base64("name;city") });

    // WHEN each goes through intake
    const first = intakeAttachment(latin);
    const second = intakeAttachment(utf8);

    // THEN the first is read by the runtime, the second travels as text
    expect(first.accepted && first.attachment.form).toBe("document");
    expect(second.accepted && second.attachment.form).toBe("inline_text");
  });

  it("refuses a fifth document, and one past the ceiling, naming the cause", () => {
    // GIVEN a fifth document for a message, and a document a byte too heavy
    const fifth = intakeAttachment(document(), { queuedDocuments: MAX_DOCUMENTS_PER_MESSAGE });
    const heavy = intakeAttachment(document({ size: MAX_DOCUMENT_BYTES + 1, base64: undefined }));

    // WHEN each goes through intake
    // THEN each is refused for its own reason
    expect(fifth.accepted === false && fifth.reason).toBe("document_limit");
    expect(heavy.accepted === false && heavy.reason).toBe("document_too_large");
  });

  it("sends a dropped document past a ceiling as its path, which the file tools read", () => {
    // GIVEN a dropped document a byte too heavy, so it carries a path and no bytes
    const input = document({
      size: MAX_DOCUMENT_BYTES + 1,
      base64: undefined,
      absolutePath: "/Users/x/invoices.pdf",
    });

    // WHEN it goes through intake
    const intake = intakeAttachment(input);

    // THEN it travels as a path rather than being refused
    expect(intake.accepted && intake.attachment.form).toBe("path_reference");
  });

  it("names the cause when it refuses a format that is not read, in both languages", () => {
    // GIVEN the refusal of a binary of a format nothing reads
    const key = refusalMessageKey("binary_not_text");

    // WHEN its sentence is read in each catalogue
    const en = lookup(enCatalogue, key);
    const fr = lookup(frCatalogue, key);

    // THEN each names the cause and the formats that are read, in its own words
    expect(en).toContain("this format is not read");
    expect(fr).toContain("PDF, Word, PowerPoint, Excel, CSV");
    expect(fr).not.toBe(en);
  });

  it("adds nothing to the text and travels beside it as its name and bytes", () => {
    // GIVEN a queued document beside a typed question
    const intake = intakeAttachment(document());
    if (!intake.accepted) throw new Error("the document was refused");
    const queued: PendingAttachment[] = [intake.attachment];

    // WHEN the message is composed
    const payload = composeUserPayload("What is the amount?", queued);

    // THEN the text is the question alone, and the document travels beside it
    expect(payload).toBe("What is the amount?");
    expect(documentPayloads(queued)).toEqual([
      { name: "invoices.pdf", data: pdfHeaderBase64() },
    ]);
    expect(imagePayloads(queued)).toEqual([]);
    expect(sentAttachmentsMetadata(queued)).toEqual({
      documents: [{ file: "", name: "invoices.pdf", size: 307_200 }],
    });
  });
});
