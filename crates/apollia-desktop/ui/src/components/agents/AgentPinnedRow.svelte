<script lang="ts">
  /**
   * AgentPinnedRow - the pinned system agent (Apollia Chat) at the top of the
   * assistants listbox. Always present, always "on"; selecting it swaps the
   * detail pane to the Apollia Chat config panel.
   */
  import { t } from "svelte-i18n";
  import { Zap } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    selected: boolean;
    onSelect: () => void;
  }

  let { selected, onSelect }: Props = $props();
</script>

<div role="option" aria-selected={selected} class="mb-0.5">
  <Button
    variant="ghost"
    size="auto"
    type="button"
    data-nav-row
    onclick={onSelect}
    class="flex w-full items-center gap-2.5 rounded-lg px-2.5 py-2 text-left transition-colors {selected
      ? 'bg-primary/10'
      : 'hover:bg-surface-1'}"
    data-testid="apollia-chat-pinned"
  >
    <span
      class="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-gradient-primary text-primary-foreground shadow-sm"
    >
      <Zap size={13} />
    </span>
    <span class="min-w-0 flex-1">
      <span
        class="flex items-center gap-1.5 text-body-xs text-foreground {selected
          ? 'font-semibold'
          : 'font-medium'}"
      >
        <span class="truncate">Apollia Chat</span>
      </span>
      <span class="block truncate text-caption text-muted-foreground">
        {$t("agents.apollia_chat_subtitle")}
      </span>
    </span>
  </Button>
</div>
