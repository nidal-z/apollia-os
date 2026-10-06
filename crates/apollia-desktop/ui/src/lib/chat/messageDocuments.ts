/**
 * The documents a user message carries, as its metadata lists them.
 *
 * The runtime reads each attached document on reception and writes its text
 * after what the user typed, in the content of the message. Its metadata lists
 * the documents (`documents`) and where the typed text ends
 * (`documents_start`, in UTF-16 code units, the unit of a JavaScript string).
 * A message just sent and not reloaded yet lists its documents by name only.
 */

export interface MessageDocumentRef {
  /** File name as the user sent it. */
  name: string;
  size: number;
  /** What its parts are: `page`, `slide`, `sheet` or `document`. Absent
   *  until the runtime has read the document. */
  unit?: string;
  /** Number of pages, slides or sheets. */
  parts?: number;
  /** Characters of text read from it. */
  characters?: number;
  /** Whether the text travels in the message; `false` when the agent reads
   *  the document on demand. */
  inline?: boolean;
}

function isRef(value: unknown): value is MessageDocumentRef {
  if (!value || typeof value !== "object") return false;
  const v = value as Record<string, unknown>;
  return typeof v.name === "string" && typeof v.size === "number";
}

/** The documents listed by a message metadata, in order; empty when none. */
export function messageDocuments(
  metadata: Record<string, unknown> | null | undefined,
): MessageDocumentRef[] {
  const listed = metadata?.documents;
  return Array.isArray(listed) ? listed.filter(isRef) : [];
}

/** Where the typed text ends in `content`; its whole length without documents. */
function documentsStart(
  content: string,
  metadata: Record<string, unknown> | null | undefined,
): number {
  const start = metadata?.documents_start;
  if (typeof start !== "number" || !Number.isInteger(start)) return content.length;
  return Math.min(Math.max(start, 0), content.length);
}

/** What the user typed: the content without the text of its documents. */
export function typedText(
  content: string,
  metadata: Record<string, unknown> | null | undefined,
): string {
  return content.slice(0, documentsStart(content, metadata));
}

/** The text of the documents, as the runtime wrote it after the typed text. */
export function documentsText(
  content: string,
  metadata: Record<string, unknown> | null | undefined,
): string {
  return content.slice(documentsStart(content, metadata));
}

/** The catalogue key of a label and the count it interpolates. */
export interface DocumentLabel {
  key: string;
  values?: { count: number };
}

const READ_KEYS: Record<string, string> = {
  page: "chat.documents.read_pages",
  slide: "chat.documents.read_slides",
  sheet: "chat.documents.read_sheets",
};

const ON_DEMAND_KEYS: Record<string, string> = {
  page: "chat.documents.on_demand_pages",
  slide: "chat.documents.on_demand_slides",
  sheet: "chat.documents.on_demand_sheets",
};

/**
 * How a document is described beside its name: what it holds (its pages,
 * slides or sheets), and whether its text is in the message, is read on
 * demand by the agent, or was not found. `null` while the runtime has not
 * read it yet.
 */
export function documentLabel(document: MessageDocumentRef): DocumentLabel | null {
  if (document.unit === undefined || document.parts === undefined) return null;
  if (document.characters === 0) return { key: "chat.documents.no_text" };
  const onDemand = document.inline === false;
  const counted = (onDemand ? ON_DEMAND_KEYS : READ_KEYS)[document.unit];
  if (counted) return { key: counted, values: { count: document.parts } };
  return { key: onDemand ? "chat.documents.on_demand_whole" : "chat.documents.read_whole" };
}
