<!--
  The documents a user message carries, one chip each above its text: the
  file name, then what the runtime read of it (its pages, slides or sheets,
  and whether its text is in the message or read on demand).
-->
<script lang="ts">
  import { t } from "svelte-i18n";
  import { FileText } from "lucide-svelte";
  import {
    type MessageDocumentRef,
    documentLabel,
    messageDocuments,
  } from "$lib/chat/messageDocuments";

  interface Props {
    messageId: string;
    metadata?: Record<string, unknown> | null;
  }

  let { messageId, metadata = null }: Props = $props();

  const documents = $derived(messageDocuments(metadata));

  function describe(document: MessageDocumentRef): string {
    const label = documentLabel(document);
    return label ? $t(label.key, { values: label.values }) : "";
  }
</script>

{#if documents.length > 0}
  <div
    class="flex max-w-full flex-wrap justify-end gap-1.5"
    data-testid="chat-message-documents-{messageId}"
  >
    {#each documents as document, index (index)}
      {@const description = describe(document)}
      <span
        class="inline-flex max-w-full items-center gap-1.5 rounded-md border border-border bg-background px-2 py-1 text-body-sm text-foreground"
        data-testid="chat-message-document-{messageId}-{index}"
      >
        <FileText size={14} class="shrink-0 text-muted-foreground" />
        <span class="truncate font-medium" title={document.name}>{document.name}</span>
        {#if description}
          <span class="shrink-0 text-caption text-muted-foreground">{description}</span>
        {/if}
      </span>
    {/each}
  </div>
{/if}
