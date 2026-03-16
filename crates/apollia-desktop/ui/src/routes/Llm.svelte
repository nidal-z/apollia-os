<script lang="ts">
  import { t } from "svelte-i18n";
  import { llmBackends } from "$lib/stores/sse";
  import { uiMode } from "$lib/stores/mode";
  import { currentRoute } from "$lib/stores/navigation";
  import LlmBackendCard from "../components/llm/LlmBackendCard.svelte";
  import LlmStats from "../components/llm/LlmStats.svelte";

  const isOperator = $derived($uiMode === "operator");
</script>

<div class="space-y-6">
  <!-- Header -->
  <h1 class="text-2xl font-bold">
    {isOperator ? $t('llm.title_operator') : $t('llm.title')}
  </h1>

  <!-- Backend cards or empty state -->
  {#if $llmBackends.length === 0}
    <div class="flex flex-col items-center justify-center gap-4 rounded-lg border border-dashed py-16">
      {#if isOperator}
        <p class="text-muted-foreground font-medium">
          {$t('llm.empty_operator')}
        </p>
        <p class="text-sm text-muted-foreground">
          {$t('llm.empty_operator_hint')}
        </p>
      {:else}
        <p class="text-muted-foreground">
          {$t('llm.empty')}
        </p>
      {/if}
      <button
        type="button"
        class="text-sm text-primary underline-offset-4 hover:underline"
        onclick={() => currentRoute.set("settings")}
      >
        {$t('llm.open_settings')}
      </button>
    </div>
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
