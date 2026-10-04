import { describe, it, expect } from "vitest";
import enCatalogue from "$lib/i18n/en.json";
import frCatalogue from "$lib/i18n/fr.json";
import {
  type AttachmentCandidate,
  type AttachmentRefusal,
  type PendingAttachment,
  composeUserPayload,
  imagePayloads,
  intakeAttachment,
  refusalMessageKey,
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

  it("refuses a small PDF picked through the paperclip, whose bytes are not text", () => {
    // GIVEN a 300 kB PDF, small enough that the composer read it inline
    const input = candidate({
      name: "rapport.pdf",
      mime: "application/pdf",
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
    // GIVEN the same PDF dropped from the desktop, so it also carries a path
    const input = candidate({
      name: "rapport.pdf",
      mime: "application/pdf",
      kind: "other",
      size: 307_200,
      base64: pdfHeaderBase64(),
      absolutePath: "/Users/x/rapport.pdf",
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
