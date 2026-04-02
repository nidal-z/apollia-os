<script lang="ts">
  import { onMount } from "svelte";
  import { get } from "svelte/store";
  import { t } from "svelte-i18n";
  import { Sparkles } from "lucide-svelte";
  import { companionStore } from "$lib/stores/companion";
  import { readyLlmBackends } from "$lib/stores/llm";

  interface Props {
    /** Whether the parent sidebar is collapsed to icon-only mode. */
    collapsed?: boolean;
  }

  let { collapsed = false }: Props = $props();

  const isEnabled = $derived($companionStore.enabled);
  const hasLlm = $derived($readyLlmBackends.length > 0);

  onMount(() => {
    const isMac = navigator.platform.includes("Mac");

    function handleKeydown(event: KeyboardEvent) {
      const isModifier = isMac ? event.metaKey : event.ctrlKey;
      if (isModifier && event.key === "/") {
        event.preventDefault();
        const state = get(companionStore);
        if (state.enabled) {
          companionStore.toggleVisibility();
        }
      }
    }

    window.addEventListener("keydown", handleKeydown);
    return () => window.removeEventListener("keydown", handleKeydown);
  });

  function handleClick() {
    if (!hasLlm) return;
    companionStore.toggleCompanion();
  }
</script>

<button
  class="flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors {isEnabled
    ? 'text-secondary hover:bg-secondary/10'
    : 'text-muted-foreground hover:bg-primary/[0.04] hover:text-foreground'}"
  class:justify-center={collapsed}
  class:px-2={collapsed}
  class:opacity-50={!hasLlm}
  class:cursor-not-allowed={!hasLlm}
  onclick={handleClick}
  aria-pressed={isEnabled}
  aria-label={$t("companion.toggle_label")}
  title={!hasLlm
    ? $t("companion.no_llm_tooltip")
    : collapsed
      ? $t("companion.toggle_label")
      : undefined}
  data-testid="companion-sidebar-toggle"
>
  <Sparkles size={18} strokeWidth={1.75} class="shrink-0" />
  {#if !collapsed}
    <span>{$t("companion.toggle_label")}</span>
  {/if}
</button>
