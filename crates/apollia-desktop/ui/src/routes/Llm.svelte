<script lang="ts">
  import { t } from "svelte-i18n";
  import { llmBackends } from "$lib/stores/sse";
  import LlmBackendCard from "../components/llm/LlmBackendCard.svelte";
  import LlmStats from "../components/llm/LlmStats.svelte";
</script>

<div class="space-y-6">
  <!-- Header -->
  <h1 class="text-2xl font-bold">{$t('llm.title')}</h1>

  <!-- Backend cards or empty state -->
  {#if $llmBackends.length === 0}
    <div class="flex flex-col items-center justify-center gap-4 rounded-lg border border-dashed py-16">
      <p class="text-muted-foreground">
        {$t('llm.empty')}
      </p>
      <a
        href="/settings"
        class="text-sm text-primary underline-offset-4 hover:underline"
      >
        {$t('llm.open_settings')}
      </a>
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
