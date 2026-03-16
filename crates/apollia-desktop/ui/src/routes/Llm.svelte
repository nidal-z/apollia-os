<script lang="ts">
  import { t } from "svelte-i18n";
  import { llmBackends } from "$lib/stores/sse";
  import { uiMode } from "$lib/stores/mode";
  import { currentRoute } from "$lib/stores/navigation";
  import { Brain } from "lucide-svelte";
  import LlmBackendCard from "../components/llm/LlmBackendCard.svelte";
  import LlmStats from "../components/llm/LlmStats.svelte";
  import EmptyState from "../components/common/EmptyState.svelte";

  const isOperator = $derived($uiMode === "operator");
</script>

<div class="space-y-6">
  <!-- Header -->
  <h1 class="text-2xl font-bold">
    {isOperator ? $t('llm.title_operator') : $t('llm.title')}
  </h1>

  <!-- Backend cards or empty state -->
  {#if $llmBackends.length === 0}
    <EmptyState
      icon={Brain}
      title={isOperator ? $t('llm.empty_operator') : $t('llm.empty_title')}
      subtitle={isOperator ? $t('llm.empty_operator_hint') : $t('llm.empty_subtitle')}
      ctaLabel={$t('llm.open_settings')}
      ctaAction={() => currentRoute.set("settings")}
    />
  {:else}
    <div class="grid gap-4 sm:grid-cols-1 md:grid-cols-2">
      {#each $llmBackends as backend (backend.name)}
        <LlmBackendCard {backend} />
      {/each}
    </div>

    <!-- Session statistics -->
    <LlmStats />
  {/if}
</div>
