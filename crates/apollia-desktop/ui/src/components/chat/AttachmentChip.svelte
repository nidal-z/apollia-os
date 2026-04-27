<!--
  Preview chip for a pending chat attachment (B.4).

  Rendered above the textarea when one or more files are queued. Clicking
  the × removes the attachment from the pending list. Images show a 20 px
  thumbnail; text/other show a file-type glyph.
-->
<script lang="ts">
  import { t } from "svelte-i18n";
  import { X, FileText, Image } from "lucide-svelte";
  import type { PendingAttachment } from "$lib/chat/attachments";

  interface Props {
    attachment: PendingAttachment;
    onremove: () => void;
  }

  let { attachment, onremove }: Props = $props();

  const isImage = $derived(attachment.kind === "image");
  const sizeLabel = $derived(formatSize(attachment.size));

  function formatSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} kB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }
</script>

<div
  class="inline-flex max-w-[14rem] items-center gap-1.5 rounded-md border border-border/40 bg-card/70 px-1.5 py-1 text-[11px]"
  data-testid="chat-attachment-chip"
>
  {#if isImage && attachment.previewUrl}
    <img
      src={attachment.previewUrl}
      alt=""
      class="h-5 w-5 shrink-0 rounded object-cover"
    />
  {:else if isImage}
    <Image size={12} class="shrink-0 text-muted-foreground" />
  {:else}
    <FileText size={12} class="shrink-0 text-muted-foreground" />
  {/if}
  <span class="truncate" title={attachment.name}>{attachment.name}</span>
  <span class="shrink-0 text-muted-foreground/60">{sizeLabel}</span>
  <button
    type="button"
    onclick={onremove}
    class="shrink-0 rounded p-0.5 text-muted-foreground/60 hover:bg-muted/60 hover:text-foreground"
    aria-label={$t("chat.attachments.remove")}
    data-testid="chat-attachment-remove"
  >
    <X size={10} />
  </button>
</div>
