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
  type AttachmentRefusal,
  type ImageSupport,
  type PendingAttachment,
  attachmentId,
  classifyKind,
  intakeAttachment,
  readAsBase64,
  refusalMessageKey,
  INLINE_MAX_BYTES,
  MAX_IMAGES_PER_MESSAGE,
  MAX_IMAGE_BYTES,
} from "$lib/chat/attachments";
import {
  DEFAULT_IMAGE_MAX_SIDE,
  browserImageCodec,
  imageMimeOf,
  prepareImage,
} from "$lib/chat/imagePrepare";
import { readImageSideLimit } from "$lib/settings/modelDefaults";

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

/**
 * `imageSupport` reads what the model of the conversation does with images at
 * the moment a file arrives: `null` (not known) keeps the path-or-refuse rule.
 * `imageSideLimit` reads the largest side an image keeps (Settings > Model
 * defaults), `null` when images are not held to one; a read that fails holds
 * to the default ceiling.
 */
export function createComposerAttachments(
  imageSupport: () => ImageSupport | null = () => null,
  imageSideLimit: () => Promise<number | null> = readImageSideLimit,
): ComposerAttachments {
  let items = $state<PendingAttachment[]>([]);
  let dragOver = $state(false);

  function refuse(reason: AttachmentRefusal, name: string): void {
    addToast(
      get(t)(refusalMessageKey(reason), {
        values: { name, max: MAX_IMAGES_PER_MESSAGE, mb: MAX_IMAGE_BYTES / (1024 * 1024) },
      }),
      "error",
    );
  }

  async function ingest(files: FileList | File[]): Promise<void> {
    const list = Array.from(files);
    for (const file of list) {
      const kind = classifyKind(imageMimeOf(file.type, file.name), file.name);
      const candidate: AttachmentCandidate = {
        id: attachmentId(),
        name: file.name,
        mime: imageMimeOf(file.type, file.name),
        size: file.size,
        kind,
      };
      const support = imageSupport();
      const queuedImages = items.filter((a) => a.form === "image").length;
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
      // To a model that reads images, the image travels as its bytes, in a
      // format the engine decodes; past the ceiling it is not even read.
      if (kind === "image" && support?.vision && queuedImages < MAX_IMAGES_PER_MESSAGE) {
        const maxSide = await imageSideLimit().catch(() => DEFAULT_IMAGE_MAX_SIDE);
        const prepared = await prepareImage(
          file,
          candidate.mime,
          support.formats,
          browserImageCodec,
          maxSide,
        );
        if (!prepared.ok) {
          if (candidate.previewUrl) URL.revokeObjectURL(candidate.previewUrl);
          refuse(prepared.reason, file.name);
          continue;
        }
        candidate.mime = prepared.mime;
        candidate.base64 = prepared.base64;
      }

      const intake = intakeAttachment(candidate, {
        vision: support?.vision ?? false,
        queuedImages,
      });
      if (!intake.accepted) {
        // The file is dropped rather than queued: a chip the send would not
        // carry is what let a silently unusable turn leave the composer.
        if (candidate.previewUrl) URL.revokeObjectURL(candidate.previewUrl);
        refuse(intake.reason, intake.name);
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
