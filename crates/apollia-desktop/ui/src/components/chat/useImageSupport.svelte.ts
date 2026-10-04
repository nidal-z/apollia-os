/**
 * What the model of a conversation does with images, re-read when the
 * conversation or its model changes.
 *
 * A read that arrives after the conversation was left behind is dropped, so
 * it never writes into the one now shown. A failed read leaves `null`, which
 * the composer takes as "images not offered".
 */
import { getChatModelCapabilities } from "$lib/ipc/chat";
import type { ImageSupport } from "$lib/chat/attachments";

/** `sessionId` and `model` are read inside the effect, so they stay reactive. */
export function createImageSupport(sessionId: () => string, model: () => string | null | undefined) {
  let support = $state<ImageSupport | null>(null);
  $effect(() => {
    const id = sessionId();
    void model();
    let cancelled = false;
    getChatModelCapabilities(id)
      .then((c) => {
        if (!cancelled) support = { vision: c.vision, formats: c.image_formats };
      })
      .catch(() => {
        if (!cancelled) support = null;
      });
    return () => {
      cancelled = true;
    };
  });
  return {
    get value() {
      return support;
    },
  };
}
