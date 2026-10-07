/**
 * What the model of a conversation can do, re-read when the conversation or
 * its model changes: its reasoning setting and whether it reads images.
 *
 * A read that arrives after the conversation was left behind is dropped, so
 * it never writes into the one now shown. A failed read leaves `null`, which
 * the composer takes as "nothing offered": no chip, no image.
 */
import { getChatModelCapabilities } from "$lib/ipc/chat";
import type { ImageSupport } from "$lib/chat/attachments";
import type { ChatModelCapabilities } from "$lib/types";

/** `sessionId` and `model` are read inside the effect, so they stay reactive. */
export function createModelCapabilities(
  sessionId: () => string,
  model: () => string | null | undefined,
) {
  let capabilities = $state<ChatModelCapabilities | null>(null);
  $effect(() => {
    const id = sessionId();
    void model();
    let cancelled = false;
    getChatModelCapabilities(id)
      .then((c) => {
        if (!cancelled) capabilities = c;
      })
      .catch(() => {
        if (!cancelled) capabilities = null;
      });
    return () => {
      cancelled = true;
    };
  });
  return {
    get value(): ChatModelCapabilities | null {
      return capabilities;
    },
    /** What the model does with images; `null` while it is not known. */
    get images(): ImageSupport | null {
      return capabilities
        ? { vision: capabilities.vision, formats: capabilities.image_formats }
        : null;
    },
  };
}
