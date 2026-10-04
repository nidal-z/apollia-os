import { describe, it, expect } from "vitest";
import { type ImageCodec, prepareImage } from "./imagePrepare";

/** The formats llama-server decodes, as the runtime capability lists them. */
const LLAMA_SERVER_FORMATS_FOR_TEST = ["jpeg", "png", "tga", "bmp", "gif"];

/** A codec that decodes anything but the bytes `BROKEN`, and redraws as `PNG!`. */
const codec: ImageCodec = {
  async check(blob) {
    if ((await blob.text()) === "BROKEN") throw new Error("not an image");
  },
  async toPng() {
    return new Blob(["PNG!"], { type: "image/png" });
  },
};

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
