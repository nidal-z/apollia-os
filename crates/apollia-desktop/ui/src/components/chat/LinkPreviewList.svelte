<!--
  Renders a rich preview card for each standalone link detected in the
  markdown body of a chat message.  Inline links stay bare.
-->
<script lang="ts">
  import { detectStandaloneLinks } from "$lib/chat/linkPreview";
  import RichLinkPreview from "./RichLinkPreview.svelte";

  interface Props {
    content: string;
  }

  let { content }: Props = $props();

  const urls = $derived(detectStandaloneLinks(content));
</script>

{#if urls.length > 0}
  <div class="mt-2 space-y-1.5" data-testid="link-preview-list">
    {#each urls as url (url)}
      <RichLinkPreview {url} />
    {/each}
  </div>
{/if}
