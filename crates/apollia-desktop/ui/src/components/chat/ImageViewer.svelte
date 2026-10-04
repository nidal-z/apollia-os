<!--
  An image of a message opened in large, with its position, previous, next
  and close.
-->
<script lang="ts">
  import { t } from "svelte-i18n";
  import { ChevronLeft, ChevronRight } from "lucide-svelte";
  import { Dialog } from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import { stepIndex } from "$lib/chat/messageImages";

  interface Props {
    urls: (string | null)[];
    /** The image shown, `null` when the viewer is closed. */
    index: number | null;
    onindex: (index: number) => void;
    onclose: () => void;
  }

  let { urls, index, onindex, onclose }: Props = $props();

  const total = $derived(urls.length);
  const current = $derived(index === null ? null : (urls[index] ?? null));
</script>

<Dialog
  open={index !== null}
  {onclose}
  size="xl"
  label={$t("chat.images.viewer.label")}
  data-testid="chat-image-viewer"
>
  {#if index !== null}
    <div class="flex flex-col items-center gap-3">
      {#if current}
        <img
          src={current}
          alt={$t("chat.images.viewer.position", { values: { position: index + 1, total } })}
          class="max-h-[70vh] max-w-full rounded-md object-contain"
          data-testid="chat-image-viewer-image"
        />
      {/if}
      <div class="flex w-full items-center justify-between gap-2">
        <Button
          variant="ghost"
          size="sm"
          disabled={total < 2}
          onclick={() => onindex(stepIndex(index, total, -1))}
          data-testid="chat-image-viewer-previous"
        >
          <ChevronLeft size={14} />
          {$t("chat.images.viewer.previous")}
        </Button>
        <span class="text-body-sm text-muted-foreground" data-testid="chat-image-viewer-position">
          {$t("chat.images.viewer.position", { values: { position: index + 1, total } })}
        </span>
        <Button
          variant="ghost"
          size="sm"
          disabled={total < 2}
          onclick={() => onindex(stepIndex(index, total, 1))}
          data-testid="chat-image-viewer-next"
        >
          {$t("chat.images.viewer.next")}
          <ChevronRight size={14} />
        </Button>
      </div>
    </div>
  {/if}
</Dialog>
