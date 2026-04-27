/**
 * Pending chat attachments (B.4).
 *
 * v1: small files (< 512 kB) are read as base64 and sent inline; larger
 * files are sent as an absolute path reference (validated by the
 * filesystem HITL flow before being read by a tool).
 */
export type AttachmentKind = "image" | "text" | "other";

export interface PendingAttachment {
  /** Stable id for UI keying / removal. */
  id: string;
  name: string;
  /** MIME type as reported by the browser (best-effort). */
  mime: string;
  size: number;
  kind: AttachmentKind;
  /** Object URL for image previews — caller revokes on removal. */
  previewUrl?: string;
  /** Base64 payload when the file is small enough to inline. */
  base64?: string;
  /** Absolute path when the file was dropped from the desktop FS. */
  absolutePath?: string;
}

export const INLINE_MAX_BYTES = 512 * 1024;

export function classifyKind(mime: string, name: string): AttachmentKind {
  if (mime.startsWith("image/")) return "image";
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
