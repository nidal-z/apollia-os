<!--
  Thumbnails of the images a user message carries.

  A message just sent shows the composer's previews; a reloaded one reads each
  stored image through `read_chat_image`. A click opens the image viewer on
  that image.
-->
<script lang="ts">
  import { t } from "svelte-i18n";
  import { Image as ImageIcon } from "lucide-svelte";
  import { readChatImage } from "$lib/ipc/chat";
  import { dataUrlOf, messageImages } from "$lib/chat/messageImages";
  import ImageViewer from "./ImageViewer.svelte";

  interface Props {
    sessionId: string;
    messageId: string;
    metadata?: Record<string, unknown> | null;
  }

  let { sessionId, messageId, metadata = null }: Props = $props();

  const images = $derived(messageImages(metadata));
  let urls = $state<(string | null)[]>([]);
  let openAt = $state<number | null>(null);

  $effect(() => {
    const list = images;
    const session = sessionId;
    let cancelled = false;
    urls = list.map((image) => image.preview ?? null);
    list.forEach((image, index) => {
      if (image.preview || !image.file) return;
      readChatImage(session, image.file)
        .then((data) => {
          if (!cancelled) urls[index] = dataUrlOf(data);
        })
        .catch(() => {
          // The file is gone: the thumbnail keeps its placeholder.
        });
    });
    return () => {
      cancelled = true;
    };
  });
</script>

{#if images.length > 0}
  <div class="mb-1.5 flex flex-wrap justify-end gap-1.5" data-testid="chat-message-images-{messageId}">
    {#each images as image, index (image.file || image.preview || index)}
      <button
        type="button"
        class="h-16 w-16 overflow-hidden rounded-md border border-border bg-surface-2
          focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring"
        aria-label={$t("chat.images.open", { values: { position: index + 1 } })}
        onclick={() => (openAt = index)}
        data-testid="chat-message-image-{messageId}-{index}"
      >
        {#if urls[index]}
          <img src={urls[index]} alt="" class="h-full w-full object-cover" />
        {:else}
          <span class="flex h-full w-full items-center justify-center text-muted-foreground">
            <ImageIcon size={16} />
          </span>
        {/if}
      </button>
    {/each}
  </div>
  <ImageViewer
    {urls}
    index={openAt}
    onindex={(next) => (openAt = next)}
    onclose={() => (openAt = null)}
  />
{/if}
