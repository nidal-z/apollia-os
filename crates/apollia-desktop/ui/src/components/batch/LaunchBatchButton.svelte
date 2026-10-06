<script lang="ts">
  /**
   * LaunchBatchButton - the "Process a batch" button of an agent's header,
   * with the launch dialog it opens on that agent.
   */
  import { t } from "svelte-i18n";
  import { Layers } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import BatchLaunchDialog from "./BatchLaunchDialog.svelte";

  interface Props {
    /** Identifier of the agent. */
    agent: string;
    disabled?: boolean;
  }

  let { agent, disabled = false }: Props = $props();

  let open = $state(false);
</script>

<Button variant="outline" size="sm" {disabled} onclick={() => (open = true)} data-testid="agent-batch-btn">
  {#snippet icon()}<Layers size={12} />{/snippet}
  {$t("batch_launch.action")}
</Button>
<BatchLaunchDialog {open} {agent} onclose={() => (open = false)} />
