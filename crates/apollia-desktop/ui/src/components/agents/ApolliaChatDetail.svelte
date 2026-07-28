<script lang="ts">
  /**
   * ApolliaChatDetail - detail pane for the pinned system agent (Apollia Chat).
   * A signature-gradient header over the existing ApolliaChatConfigPanel.
   */
  import { t } from "svelte-i18n";
  import { Zap } from "lucide-svelte";
  import { DetailHeader } from "$lib/components/operator";
  import ApolliaChatConfigPanel from "./ApolliaChatConfigPanel.svelte";
</script>

<DetailHeader title="Apollia Chat" meta={$t("agents.apollia_chat_detail_subtitle")}>
  {#snippet leading()}
    <span
      class="inline-flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-gradient-primary text-primary-foreground shadow-elev-2"
    >
      <Zap size={18} />
    </span>
  {/snippet}
</DetailHeader>

<div class="px-8 pb-8 pt-5">
  <ApolliaChatConfigPanel />
</div>
