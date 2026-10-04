<script lang="ts">
  /**
   * Body of a question the agent asked: each question and the answer given,
   * or that it was skipped.
   */

  import type { ReasoningItem } from "$lib/chat/reasoning";
  import { askUserPairs } from "$lib/chat/askUserPairs";
  import { t } from "svelte-i18n";

  type ToolCallItem = Extract<ReasoningItem, { kind: "tool_call" }>;

  let { item }: { item: ToolCallItem } = $props();

  const pairs = $derived(askUserPairs(item.args, item.output) ?? []);
  const answered = $derived(pairs.filter((p) => !p.skipped && p.answer).length);
</script>

<div class="flex flex-col gap-2" data-testid="tool-body-ask_user">
  <p class="text-caption text-muted-foreground">
    {$t("chat.ask_user_qa_label")} · {answered}/{pairs.length}
  </p>
  {#each pairs as pair (pair.id)}
    <div class="flex flex-col gap-0.5">
      <p class="text-body-xs text-foreground">{pair.question}</p>
      <p
        class="border-l-2 pl-2 text-body-xs {pair.skipped
          ? 'border-border italic text-muted-foreground'
          : 'border-success/40 text-foreground'}"
      >
        {pair.skipped
          ? $t("chat.ask_user_skipped_label")
          : pair.answer || $t("chat.ask_user_pending")}
      </p>
    </div>
  {/each}
</div>
