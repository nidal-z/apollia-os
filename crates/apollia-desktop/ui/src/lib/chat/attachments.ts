/**
 * Pending chat attachments.
 *
 * A queued attachment travels to the model in exactly one of four forms, and
 * the form is decided at intake rather than at send time:
 *
 *   `inline_text`     the file was small enough to read and its decoded text
 *                     rides inside the user message.
 *   `path_reference`  the file came from a desktop drop, so an absolute path
 *                     rides instead and the file tools read it.
 *   `image`           the model of the conversation reads images: the bytes
 *                     travel beside the text, in a format its engine decodes.
 *   `document`        the file is of a format the runtime reads (PDF, Word,
 *                     PowerPoint, a workbook, a CSV file that is not UTF-8):
 *                     its bytes travel beside the text and the runtime
 *                     extracts the text on reception.
 *
 * Anything that fits none of them is refused at intake, with a reason the
 * composer shows. An image to a model that does not read images is refused
 * (or sent as a path when dropped), a file picked through the paperclip past
 * `INLINE_MAX_BYTES` carries neither payload nor path, because only a Tauri
 * drop event exposes `path` on a `File`, and a binary of a format nothing
 * reads has no decoded text at all, so `inline_text` would carry replacement
 * characters where the form promises readable content.
 */
export type AttachmentKind = "image" | "text" | "document" | "other";

/** The single shape an accepted attachment takes inside the user message. */
export type AttachmentForm = "inline_text" | "path_reference" | "image" | "document";

/** Why the composer refuses a file instead of queueing it. */
export type AttachmentRefusal =
  | "read_failed"
  | "image_not_supported"
  | "image_limit"
  | "image_too_large"
  | "image_unreadable"
  | "image_format"
  | "too_large_without_path"
  | "document_limit"
  | "document_too_large"
  | "binary_not_text";

/** Most images one message carries; the runtime holds the same ceiling. */
export const MAX_IMAGES_PER_MESSAGE = 8;

/** Largest image sent, in bytes (10 MiB); the runtime holds the same ceiling. */
export const MAX_IMAGE_BYTES = 10 * 1024 * 1024;

/** Most documents one message carries; the runtime holds the same ceiling. */
export const MAX_DOCUMENTS_PER_MESSAGE = 4;

/** Largest document sent, in bytes (20 MiB); the runtime holds the same ceiling. */
export const MAX_DOCUMENT_BYTES = 20 * 1024 * 1024;

/** Extensions of the formats the runtime reads as documents, lower case. */
export const DOCUMENT_EXTENSIONS: readonly string[] = [
  "pdf",
  "docx",
  "docm",
  "pptx",
  "pptm",
  "xlsx",
  "xlsm",
  "xlsb",
  "xls",
  "ods",
];

/** The extension of a file name, lower case; empty without one. */
function extensionOf(name: string): string {
  const dot = name.lastIndexOf(".");
  return dot < 0 ? "" : name.slice(dot + 1).toLowerCase();
}

/** One document as it travels to the runtime. */
export interface DocumentPayload {
  name: string;
  /** Bytes in standard base64. */
  data: string;
}

/** What the model of the conversation reads, as far as images go. */
export interface ImageSupport {
  vision: boolean;
  /** Formats its engine decodes (`png`, `jpeg`...); empty means any. */
  formats: readonly string[];
}

/** One image as it travels to the runtime. */
export interface ImagePayload {
  mime_type: string;
  /** Bytes in standard base64. */
  data: string;
}

export interface PendingAttachment {
  /** Stable id for UI keying / removal. */
  id: string;
  name: string;
  /** MIME type as reported by the browser (best-effort). */
  mime: string;
  size: number;
  kind: AttachmentKind;
  /** How this attachment will be rendered into the user message. */
  form: AttachmentForm;
  /** Object URL for image previews - caller revokes on removal. */
  previewUrl?: string;
  /** Base64 payload. Always present when `form` is `inline_text`, `image` or
   *  `document`. */
  base64?: string;
  /** Absolute path. Always present when `form` is `path_reference`. */
  absolutePath?: string;
}

export const INLINE_MAX_BYTES = 512 * 1024;

export function classifyKind(mime: string, name: string): AttachmentKind {
  if (mime.startsWith("image/")) return "image";
  if (DOCUMENT_EXTENSIONS.includes(extensionOf(name))) return "document";
  if (mime.startsWith("text/") || /\.(md|txt|json|ya?ml|rs|ts|tsx|js|jsx|py|toml|csv)$/i.test(name))
    return "text";
  return "other";
}

/** Read a File as base64 (without the data-URI prefix). */
export function readAsBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(reader.error ?? new Error("read failed"));
    reader.onload = () => {
      const result = reader.result;
      if (typeof result !== "string") {
        reject(new Error("unexpected reader result"));
        return;
      }
      const comma = result.indexOf(",");
      resolve(comma >= 0 ? result.slice(comma + 1) : result);
    };
    reader.readAsDataURL(file);
  });
}

/** Generate a random id without depending on crypto.randomUUID in tests. */
export function attachmentId(): string {
  return `att-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

/** What the composer knows about a file once the read attempt has settled. */
export interface AttachmentCandidate {
  id: string;
  name: string;
  mime: string;
  size: number;
  kind: AttachmentKind;
  previewUrl?: string;
  /** Base64 payload when the inline read succeeded. */
  base64?: string;
  /** Absolute path when the file arrived through a desktop drop. */
  absolutePath?: string;
  /** True when an inline read was attempted and threw. */
  readFailed?: boolean;
}

export type AttachmentIntake =
  | { accepted: true; attachment: PendingAttachment }
  | { accepted: false; reason: AttachmentRefusal; name: string };

/**
 * True when a payload decodes to text rather than to replacement characters.
 *
 * `inline_text` promises decoded text inside the user message, so bytes that
 * are not text may not take that form. A strict UTF-8 decode throws on them,
 * and an embedded NUL catches the encodings that survive the decode.
 */
function decodesAsText(b64: string): boolean {
  try {
    const bin = atob(b64);
    const bytes = Uint8Array.from(bin, (c) => c.charCodeAt(0));
    return !new TextDecoder("utf-8", { fatal: true }).decode(bytes).includes("\u0000");
  } catch {
    return false;
  }
}

/** Bytes a base64 payload decodes to, without decoding it. */
export function base64ByteLength(b64: string): number {
  const padding = b64.endsWith("==") ? 2 : b64.endsWith("=") ? 1 : 0;
  return Math.floor((b64.length * 3) / 4) - padding;
}

/** What intake knows of the conversation when a file arrives. */
export interface IntakeContext {
  /** The model reads images; absent or false keeps the path-or-refuse rule. */
  vision?: boolean;
  /** Images already queued for this message. */
  queuedImages?: number;
  /** Documents already queued for this message. */
  queuedDocuments?: number;
}

/**
 * Decide whether a candidate can be queued, and under which form.
 *
 * To a model that reads images, an image travels as its bytes, up to
 * `MAX_IMAGES_PER_MESSAGE` per message and `MAX_IMAGE_BYTES` each. To any
 * other, an image only travels as a path. A document of a format the runtime
 * reads travels as its bytes, up to `MAX_DOCUMENTS_PER_MESSAGE` per message
 * and `MAX_DOCUMENT_BYTES` each; past a ceiling it falls back to its path,
 * which the file tools read. Everything else prefers its inline payload,
 * provided that payload is really text, and falls back to a path. A binary of
 * a format nothing reads is refused.
 */
export function intakeAttachment(
  candidate: AttachmentCandidate,
  context: IntakeContext = {},
): AttachmentIntake {
  const accept = (form: AttachmentForm): AttachmentIntake => ({
    accepted: true,
    attachment: {
      id: candidate.id,
      name: candidate.name,
      mime: candidate.mime,
      size: candidate.size,
      kind: candidate.kind,
      form,
      previewUrl: candidate.previewUrl,
      base64: candidate.base64,
      absolutePath: candidate.absolutePath,
    },
  });

  if (candidate.kind === "image" && context.vision) {
    const refuse = (reason: AttachmentRefusal): AttachmentIntake => ({
      accepted: false,
      reason,
      name: candidate.name,
    });
    if ((context.queuedImages ?? 0) >= MAX_IMAGES_PER_MESSAGE) return refuse("image_limit");
    if (candidate.base64 === undefined) return refuse("image_unreadable");
    if (base64ByteLength(candidate.base64) > MAX_IMAGE_BYTES) return refuse("image_too_large");
    return accept("image");
  }
  if (candidate.kind === "image") {
    return candidate.absolutePath
      ? accept("path_reference")
      : { accepted: false, reason: "image_not_supported", name: candidate.name };
  }
  const refuseDocument = (reason: AttachmentRefusal): AttachmentIntake =>
    candidate.absolutePath
      ? accept("path_reference")
      : { accepted: false, reason, name: candidate.name };
  const acceptDocument = (): AttachmentIntake => {
    if ((context.queuedDocuments ?? 0) >= MAX_DOCUMENTS_PER_MESSAGE) {
      return refuseDocument("document_limit");
    }
    return accept("document");
  };
  if (candidate.kind === "document") {
    if (candidate.size > MAX_DOCUMENT_BYTES) return refuseDocument("document_too_large");
    if (candidate.base64 !== undefined) return acceptDocument();
    if (candidate.absolutePath) return accept("path_reference");
    return { accepted: false, reason: "read_failed", name: candidate.name };
  }
  if (candidate.base64 !== undefined && decodesAsText(candidate.base64)) {
    return accept("inline_text");
  }
  // A CSV file saved in another encoding is still a table the runtime reads.
  if (candidate.base64 !== undefined && extensionOf(candidate.name) === "csv") {
    return acceptDocument();
  }
  if (candidate.absolutePath) return accept("path_reference");
  if (candidate.base64 !== undefined) {
    return { accepted: false, reason: "binary_not_text", name: candidate.name };
  }
  if (candidate.readFailed) {
    return { accepted: false, reason: "read_failed", name: candidate.name };
  }
  return { accepted: false, reason: "too_large_without_path", name: candidate.name };
}

/** Catalogue key carrying the sentence the user reads for a refusal. */
export function refusalMessageKey(reason: AttachmentRefusal): string {
  switch (reason) {
    case "read_failed":
      return "chat.attachments.refused.read_failed";
    case "image_not_supported":
      return "chat.attachments.refused.image_not_supported";
    case "image_limit":
      return "chat.attachments.refused.image_limit";
    case "image_too_large":
      return "chat.attachments.refused.image_too_large";
    case "image_unreadable":
      return "chat.attachments.refused.image_unreadable";
    case "image_format":
      return "chat.attachments.refused.image_format";
    case "too_large_without_path":
      return "chat.attachments.refused.too_large_without_path";
    case "document_limit":
      return "chat.attachments.refused.document_limit";
    case "document_too_large":
      return "chat.attachments.refused.document_too_large";
    case "binary_not_text":
      return "chat.attachments.refused.binary_not_text";
  }
}

function decodeBase64Utf8(b64: string): string {
  try {
    const bin = atob(b64);
    const bytes = Uint8Array.from(bin, (c) => c.charCodeAt(0));
    return new TextDecoder("utf-8", { fatal: false }).decode(bytes);
  } catch {
    return b64;
  }
}

/**
 * Render one queued attachment into the user message.
 *
 * Total over `AttachmentForm`: there is no branch that emits a tag carrying
 * neither content nor path, which is the shape the model could not act on. An
 * image and a document add nothing to the text: their bytes travel beside it,
 * and the runtime writes the text of a document into the message.
 */
function composeAttachmentPart(att: PendingAttachment): string {
  switch (att.form) {
    case "image":
    case "document":
      return "";
    case "inline_text":
      return `\n<attachment name="${att.name}" mime="${att.mime}" size="${att.size}">\n${decodeBase64Utf8(att.base64 ?? "")}\n</attachment>`;
    case "path_reference":
      return `\n<attachment name="${att.name}" path="${att.absolutePath ?? ""}" size="${att.size}" />`;
  }
}

/** Build the message body sent to the backend, text first then attachments. */
export function composeUserPayload(
  content: string,
  attachments: readonly PendingAttachment[],
): string {
  if (attachments.length === 0) return content;
  const parts: string[] = [];
  if (content.trim()) parts.push(content);
  for (const att of attachments) parts.push(composeAttachmentPart(att));
  return parts.join("");
}

/** The images of a message, in order, as the runtime receives them. */
export function imagePayloads(attachments: readonly PendingAttachment[]): ImagePayload[] {
  return attachments
    .filter((att) => att.form === "image" && att.base64 !== undefined)
    .map((att) => ({ mime_type: att.mime, data: att.base64 ?? "" }));
}

/**
 * The metadata a message just sent carries until the history is reloaded:
 * the composer's previews, so its thumbnails show at once; `null` without
 * images.
 */
export function sentImagesMetadata(
  attachments: readonly PendingAttachment[],
): Record<string, unknown> | null {
  const images = attachments
    .filter((att) => att.form === "image")
    .map((att) => ({ file: "", mime_type: att.mime, size: att.size, preview: att.previewUrl }));
  return images.length > 0 ? { images } : null;
}

/** The documents of a message, in order, as the runtime receives them. */
export function documentPayloads(attachments: readonly PendingAttachment[]): DocumentPayload[] {
  return attachments
    .filter((att) => att.form === "document" && att.base64 !== undefined)
    .map((att) => ({ name: att.name, data: att.base64 ?? "" }));
}

/**
 * The metadata a message just sent carries until the history is reloaded:
 * its images and its documents by name; `null` without either. What the
 * runtime read of a document (pages, text) is only known once reloaded.
 */
export function sentAttachmentsMetadata(
  attachments: readonly PendingAttachment[],
): Record<string, unknown> | null {
  const documents = attachments
    .filter((att) => att.form === "document")
    .map((att) => ({ file: "", name: att.name, size: att.size }));
  const images = sentImagesMetadata(attachments);
  if (documents.length === 0) return images;
  return { ...(images ?? {}), documents };
}
