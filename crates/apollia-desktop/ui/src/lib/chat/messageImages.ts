/**
 * The images a user message carries, as its metadata lists them.
 *
 * The runtime writes `metadata.images`, one entry per stored file. A message
 * just sent and not reloaded yet carries the composer's previews instead
 * (`preview`), so its thumbnails show before the history is read back.
 */
import type { ChatImageData } from "$lib/types";

export interface MessageImageRef {
  /** File name in the folder of the conversation; empty for a local preview. */
  file: string;
  mime_type: string;
  size: number;
  /** Object URL of the composer's preview, for a message not reloaded yet. */
  preview?: string;
}

function isRef(value: unknown): value is MessageImageRef {
  if (!value || typeof value !== "object") return false;
  const v = value as Record<string, unknown>;
  return (
    typeof v.file === "string" &&
    typeof v.mime_type === "string" &&
    typeof v.size === "number" &&
    (v.file !== "" || typeof v.preview === "string")
  );
}

/** The images listed by a message metadata, in order; empty when none. */
export function messageImages(
  metadata: Record<string, unknown> | null | undefined,
): MessageImageRef[] {
  const listed = metadata?.images;
  return Array.isArray(listed) ? listed.filter(isRef) : [];
}

/** A stored image as a URL an `<img>` shows. */
export function dataUrlOf(image: ChatImageData): string {
  return `data:${image.mime_type};base64,${image.data}`;
}

/** The position after a step of `delta` in a list of `total`, wrapping. */
export function stepIndex(index: number, total: number, delta: number): number {
  if (total <= 0) return 0;
  return (((index + delta) % total) + total) % total;
}
