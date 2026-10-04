import { describe, it, expect } from "vitest";
import { type ImageCodec, type ImageSize, fitWithin, prepareImage } from "./imagePrepare";

/** The formats llama-server decodes, as the runtime capability lists them. */
const LLAMA_SERVER_FORMATS_FOR_TEST = ["jpeg", "png", "tga", "bmp", "gif"];

/** A codec that decodes anything but the bytes `BROKEN`, and redraws as `PNG!`. */
const codec: ImageCodec = {
  async check(blob) {
    if ((await blob.text()) === "BROKEN") throw new Error("not an image");
  },
  async size(blob) {
    return sizeOf(await blob.text());
  },
  async toPng(_blob, size) {
    return new Blob([size ? `PNG ${size.width}x${size.height}` : "PNG!"], { type: "image/png" });
  },
};

/** The size a test image declares in its bytes (`4000x3000`), else 800 by 600. */
function sizeOf(bytes: string): ImageSize {
  const declared = /^(\d+)x(\d+)$/.exec(bytes);
  return declared
    ? { width: Number(declared[1]), height: Number(declared[2]) }
    : { width: 800, height: 600 };
}

const b64 = (text: string): string => Buffer.from(text).toString("base64");

describe("prepareImage, an image made ready for the engine", () => {
  it("sends a PNG as is to llama-server", async () => {
    // GIVEN a PNG and an engine that reads PNG
    const png = new Blob(["PNGDATA"], { type: "image/png" });

    // WHEN it is prepared
    const prepared = await prepareImage(png, "image/png", LLAMA_SERVER_FORMATS_FOR_TEST, codec);

    // THEN its bytes and type are unchanged
    expect(prepared).toEqual({ ok: true, mime: "image/png", base64: b64("PNGDATA") });
  });

  it("converts a WebP to PNG for llama-server, which does not decode WebP", async () => {
    // GIVEN a WebP the web view decodes
    const webp = new Blob(["WEBPDATA"], { type: "image/webp" });

    // WHEN it is prepared for llama-server
    const prepared = await prepareImage(webp, "image/webp", LLAMA_SERVER_FORMATS_FOR_TEST, codec);

    // THEN it travels as PNG
    expect(prepared).toEqual({ ok: true, mime: "image/png", base64: b64("PNG!") });
  });

  it("sends a WebP as is to an engine that reads WebP", async () => {
    // GIVEN a WebP and the MLX engine's formats
    const webp = new Blob(["WEBPDATA"], { type: "image/webp" });

    // WHEN it is prepared
    const prepared = await prepareImage(webp, "image/webp", ["png", "jpeg", "webp"], codec);

    // THEN it is not converted
    expect(prepared).toEqual({ ok: true, mime: "image/webp", base64: b64("WEBPDATA") });
  });

  it("refuses a corrupted file the web view cannot decode", async () => {
    // GIVEN a file named .png whose bytes are no image
    const broken = new Blob(["BROKEN"], { type: "image/png" });

    // WHEN it is prepared
    const prepared = await prepareImage(broken, "image/png", LLAMA_SERVER_FORMATS_FOR_TEST, codec);

    // THEN it is refused as unreadable
    expect(prepared).toEqual({ ok: false, reason: "image_unreadable" });
  });
});

describe("the ceiling on the largest side of an image", () => {
  it("brings the largest side of a 4000 pixel image down to 2048, proportions kept", async () => {
    // GIVEN an image of 4000 by 3000 pixels and a ceiling of 2048
    const large = new Blob(["4000x3000"], { type: "image/jpeg" });

    // WHEN the message is prepared
    const prepared = await prepareImage(large, "image/jpeg", LLAMA_SERVER_FORMATS_FOR_TEST, codec, 2048);

    // THEN its largest side is 2048 and the other follows
    expect(prepared).toEqual({ ok: true, mime: "image/png", base64: b64("PNG 2048x1536") });
    expect(fitWithin({ width: 3000, height: 4000 }, 2048)).toEqual({ width: 1536, height: 2048 });
  });

  it("leaves an image under the ceiling as it is", async () => {
    // GIVEN an image of 800 by 600 pixels and a ceiling of 2048
    const small = new Blob(["800x600"], { type: "image/png" });

    // WHEN it is prepared
    const prepared = await prepareImage(small, "image/png", LLAMA_SERVER_FORMATS_FOR_TEST, codec, 2048);

    // THEN its bytes and type are unchanged
    expect(prepared).toEqual({ ok: true, mime: "image/png", base64: b64("800x600") });
  });

  it("does not resize when the limit is switched off", async () => {
    // GIVEN an image of 4000 by 3000 pixels and no ceiling
    const large = new Blob(["4000x3000"], { type: "image/jpeg" });

    // WHEN it is prepared without a ceiling
    const prepared = await prepareImage(large, "image/jpeg", LLAMA_SERVER_FORMATS_FOR_TEST, codec, null);

    // THEN it travels at its own size
    expect(prepared).toEqual({ ok: true, mime: "image/jpeg", base64: b64("4000x3000") });
  });

  it("keeps a thin image at least one pixel wide", () => {
    // GIVEN a strip of 10000 by 2 pixels
    // WHEN it is fitted within 2048
    const fitted = fitWithin({ width: 10_000, height: 2 }, 2048);

    // THEN its short side does not vanish
    expect(fitted).toEqual({ width: 2048, height: 1 });
  });
});
