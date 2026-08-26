/**
 * The files a composer turn carries.
 *
 * Owns the queued attachments and the drag state, and refuses a file the intake
 * rules reject rather than showing a chip the send would not carry. The rules
 * themselves are pure and live in `$lib/chat/attachments`; what this factory
 * adds is the live list, so the composer component keeps only the markup.
 */
import { get } from "svelte/store";
import { t } from "svelte-i18n";
import { addToast } from "$lib/components/ui/toast";
import {
  type AttachmentCandidate,
  type PendingAttachment,
  attachmentId,
  classifyKind,
  intakeAttachment,
  readAsBase64,
  refusalMessageKey,
  INLINE_MAX_BYTES,
} from "$lib/chat/attachments";

export interface ComposerAttachments {
  /** The queue, in the order the operator built it. */
  readonly items: PendingAttachment[];
  /** True while a drag hovers the composer card. */
  readonly dragOver: boolean;
  ingest(files: FileList | File[]): Promise<void>;
  remove(id: string): void;
  /** Empties the queue after a send, without revoking the preview URLs the
   *  sent turn still renders. */
  takeAll(): PendingAttachment[];
  handleFileInput(event: Event): void;
  handleDragOver(event: DragEvent): void;
  handleDragLeave(event: DragEvent): void;
  handleDrop(event: DragEvent): void;
}

export function createComposerAttachments(): ComposerAttachments {
  let items = $state<PendingAttachment[]>([]);
  let dragOver = $state(false);

  async function ingest(files: FileList | File[]): Promise<void> {
    const list = Array.from(files);
    for (const file of list) {
      const kind = classifyKind(file.type, file.name);
      const candidate: AttachmentCandidate = {
        id: attachmentId(),
        name: file.name,
        mime: file.type || "application/octet-stream",
        size: file.size,
        kind,
      };
      if (kind === "image") {
        candidate.previewUrl = URL.createObjectURL(file);
      }
      // Desktop drop events expose `path` on the File (Tauri); a paperclip
      // pick never does. The path is taken whatever the size, because it is
      // the only form an image can travel under. Intake still prefers the
      // inline payload for everything else, so a dropped small text file
      // keeps travelling as text and needs no approval.
      const dropPath: unknown = (file as File & { path?: unknown }).path;
      if (typeof dropPath === "string" && dropPath) candidate.absolutePath = dropPath;

      if (kind !== "image" && file.size <= INLINE_MAX_BYTES) {
        try {
          candidate.base64 = await readAsBase64(file);
        } catch {
          candidate.readFailed = true;
        }
      }

      const intake = intakeAttachment(candidate);
      if (!intake.accepted) {
        // The file is dropped rather than queued: a chip the send would not
        // carry is what let a silently unusable turn leave the composer.
        if (candidate.previewUrl) URL.revokeObjectURL(candidate.previewUrl);
        addToast(
          get(t)(refusalMessageKey(intake.reason), { values: { name: intake.name } }),
          "error",
        );
        continue;
      }
      items = [...items, intake.attachment];
    }
  }

  function remove(id: string): void {
    const target = items.find((a) => a.id === id);
    if (target?.previewUrl) URL.revokeObjectURL(target.previewUrl);
    items = items.filter((a) => a.id !== id);
  }

  return {
    get items() {
      return items;
    },
    get dragOver() {
      return dragOver;
    },
    ingest,
    remove,
    takeAll(): PendingAttachment[] {
      const payload = items;
      items = [];
      return payload;
    },
    handleFileInput(event: Event): void {
      const input = event.target as HTMLInputElement;
      if (input.files && input.files.length > 0) {
        void ingest(input.files);
      }
      input.value = "";
    },
    handleDragOver(event: DragEvent): void {
      event.preventDefault();
      dragOver = true;
    },
    handleDragLeave(event: DragEvent): void {
      event.preventDefault();
      dragOver = false;
    },
    handleDrop(event: DragEvent): void {
      event.preventDefault();
      dragOver = false;
      const files = event.dataTransfer?.files;
      if (files && files.length > 0) void ingest(files);
    },
  };
}
