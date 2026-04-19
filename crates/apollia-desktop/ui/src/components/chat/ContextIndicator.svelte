<script lang="ts">
  import { t } from "svelte-i18n";
  import { Brain } from "lucide-svelte";
  import { uiMode } from "$lib/stores/mode";

  interface Props {
    memoryEntryCount: number;
    isInjected: boolean;
  }

  let { memoryEntryCount, isInjected }: Props = $props();

  const visible = $derived(isInjected && memoryEntryCount > 0);

  const label = $derived(
    $uiMode === "builder"
      ? $t("chat.context_injected_builder", { values: { count: memoryEntryCount } })
      : $t("chat.context_loaded"),
  );
</script>

{#if visible}
  <div
    class="flex items-center gap-1.5 px-3 py-1 animate-fade-in"
    data-testid="context-indicator"
    title={$t("chat.context_tooltip")}
  >
    <Brain size={12} class="text-secondary/60" />
    <span class="text-[10px] text-secondary/60">{label}</span>
  </div>
{/if}
