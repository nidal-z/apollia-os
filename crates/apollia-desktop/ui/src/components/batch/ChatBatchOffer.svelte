<script lang="ts">
  /**
   * ChatBatchOffer - "Process each row", offered on a workbook or a CSV file
   * joined to the chat, with the launch dialog it opens on that file.
   *
   * The batch runs the agent of the conversation; a conversation held with a
   * model alone lets the dialog ask which agent runs it. A launched batch
   * opens in its view.
   */
  import { t } from "svelte-i18n";
  import { Layers } from "lucide-svelte";
  import type { PendingAttachment } from "$lib/chat/attachments";
  import { Button } from "$lib/components/ui/button";
  import { agents } from "$lib/stores/sse";
  import { openBatch } from "$lib/stores/batchView";
  import BatchLaunchDialog from "./BatchLaunchDialog.svelte";
  import { batchFileOf } from "./launchBatch";

  interface Props {
    /** The files queued in the composer. */
    attachments: PendingAttachment[];
    /** The agent of the conversation, when it has one. */
    agentName?: string | null;
  }

  let { attachments, agentName = null }: Props = $props();

  const file = $derived(
    attachments.map((attachment) => batchFileOf(attachment)).find((path) => path !== null) ?? null,
  );
  const agent = $derived(
    agentName !== null && $agents.some((known) => known.name === agentName) ? agentName : null,
  );

  let opened = $state<string | null>(null);
</script>

{#if file}
  <Button variant="outline" size="sm" onclick={() => (opened = file)} data-testid="chat-batch-offer">
    {#snippet icon()}<Layers size={12} />{/snippet}
    {$t("batch_launch.chat_action")}
  </Button>
{/if}
<BatchLaunchDialog
  open={opened !== null}
  {agent}
  file={opened}
  onclose={() => (opened = null)}
  onlaunched={(batch) => openBatch(batch.batchId, "launch")}
/>
