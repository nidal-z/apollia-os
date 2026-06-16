<!--
  Removable chip for an MCP resource pinned to the next turn.

  Rendered above the textarea, alongside attachment chips. Clicking the ×
  unpins the resource. Mirrors AttachmentChip's structure and styling.
-->
<script lang="ts">
  import { t } from "svelte-i18n";
  import { X, Database } from "lucide-svelte";
  import type { PinnedResource } from "$lib/chat/mcpResources";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    resource: PinnedResource;
    onremove: () => void;
  }

  let { resource, onremove }: Props = $props();
</script>

<div
  class="inline-flex max-w-[15rem] items-center gap-1.5 rounded-md border border-info/30 bg-info/5 px-1.5 py-1 text-[11px]"
  data-testid="chat-pinned-resource-chip"
  title={resource.uri}
>
  <Database size={12} class="shrink-0 text-info" />
  <span class="truncate">{resource.name}</span>
  <span class="shrink-0 text-muted-foreground/60">{resource.server}</span>
  <Button variant="ghost" size="sm"
    type="button"
    onclick={onremove}
    class="shrink-0 rounded p-0.5 text-muted-foreground/60 hover:bg-muted/60 hover:text-foreground"
    aria-label={$t("chat.resources.unpin")}
    data-testid="chat-pinned-resource-remove"
  >
    <X size={10} />
  </Button>
</div>
