/**
 * An image made ready for the engine of the conversation.
 *
 * The web view decodes the image first: a file it cannot decode is refused
 * with a reason. A format the engine reads leaves untouched; one it does not
 * (WebP for llama-server, whose decoder reads JPEG, PNG, TGA, BMP and GIF) is
 * redrawn and sent as PNG. The decoding and the drawing are injected, so the
 * rule is tested without a browser.
 */

/** The format name a capability lists for a media type, `null` if unknown. */
export function formatOfMime(mime: string): string | null {
  switch (mime.trim().toLowerCase()) {
    case "image/png":
      return "png";
    case "image/jpeg":
    case "image/jpg":
    case "image/pjpeg":
      return "jpeg";
    case "image/gif":
      return "gif";
    case "image/bmp":
    case "image/x-bmp":
    case "image/x-ms-bmp":
      return "bmp";
    case "image/webp":
      return "webp";
    case "image/tiff":
      return "tiff";
    case "image/x-tga":
    case "image/tga":
    case "image/x-targa":
      return "tga";
    default:
      return null;
  }
}

const EXTENSION_MIME: Record<string, string> = {
  png: "image/png",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  gif: "image/gif",
  bmp: "image/bmp",
  webp: "image/webp",
  tif: "image/tiff",
  tiff: "image/tiff",
  tga: "image/x-tga",
};

/** The media type of an image file: the browser's, else its extension's. */
export function imageMimeOf(type: string, name: string): string {
  if (type) return type;
  const extension = name.split(".").pop()?.toLowerCase() ?? "";
  return EXTENSION_MIME[extension] ?? "application/octet-stream";
}

/** Decodes an image and redraws it as PNG; both reject on a broken file. */
export interface ImageCodec {
  /** Resolves when the bytes decode to an image, rejects otherwise. */
  check(blob: Blob): Promise<void>;
  /** The image redrawn as PNG. */
  toPng(blob: Blob): Promise<Blob>;
}

export type PreparedImage =
  | { ok: true; mime: string; base64: string }
  | { ok: false; reason: "image_unreadable" | "image_format" };

/** Bytes of a blob in standard base64, without a data-URI prefix. */
export async function blobToBase64(blob: Blob): Promise<string> {
  const bytes = new Uint8Array(await blob.arrayBuffer());
  let binary = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}

/**
 * Make `blob` ready for an engine that decodes `formats` (empty: any).
 *
 * Sent as is when its format is listed, converted to PNG when the web view
 * decodes it and the engine reads PNG, refused otherwise.
 */
export async function prepareImage(
  blob: Blob,
  mime: string,
  formats: readonly string[],
  codec: ImageCodec,
): Promise<PreparedImage> {
  try {
    await codec.check(blob);
  } catch {
    return { ok: false, reason: "image_unreadable" };
  }
  const format = formatOfMime(mime);
  if (format !== null && (formats.length === 0 || formats.includes(format))) {
    return { ok: true, mime, base64: await blobToBase64(blob) };
  }
  if (formats.length > 0 && !formats.includes("png")) {
    return { ok: false, reason: "image_format" };
  }
  try {
    const png = await codec.toPng(blob);
    return { ok: true, mime: "image/png", base64: await blobToBase64(png) };
  } catch {
    return { ok: false, reason: "image_unreadable" };
  }
}

/** The codec of the web view: `createImageBitmap` and a canvas. */
export const browserImageCodec: ImageCodec = {
  async check(blob) {
    const bitmap = await createImageBitmap(blob);
    bitmap.close();
  },
  async toPng(blob) {
    const bitmap = await createImageBitmap(blob);
    try {
      const canvas = document.createElement("canvas");
      canvas.width = bitmap.width;
      canvas.height = bitmap.height;
      const context = canvas.getContext("2d");
      if (!context) throw new Error("no 2d context");
      context.drawImage(bitmap, 0, 0);
      return await new Promise<Blob>((resolve, reject) =>
        canvas.toBlob(
          (png) => (png ? resolve(png) : reject(new Error("png encoding failed"))),
          "image/png",
        ),
      );
    } finally {
      bitmap.close();
    }
  },
};
