<script lang="ts">
  import { fly } from "svelte/transition";
  import { flip } from "svelte/animate";
  import { t } from "svelte-i18n";
  import { llmBackends, connectionStatus } from "$lib/stores/sse";
  import { uiMode } from "$lib/stores/mode";
  import { currentRoute } from "$lib/stores/navigation";
  import { Brain } from "lucide-svelte";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import LlmBackendCard from "../components/llm/LlmBackendCard.svelte";
  import LlmStats from "../components/llm/LlmStats.svelte";
  import EmptyState from "../components/common/EmptyState.svelte";

  const SKELETON_COUNT = 2;

  const isOperator = $derived($uiMode === "operator");
</script>

<div class="mx-auto w-full max-w-6xl space-y-6" data-testid="llm-page">
  <!-- Header -->
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-semibold">
        {isOperator ? $t('llm.title_operator') : $t('llm.title')}
      </h1>
      <p class="text-xs text-muted-foreground" data-testid="llm-subtitle">{$t('llm.subtitle')}</p>
    </div>
  </div>

  <!-- Backend cards, skeleton loaders, or empty state -->
  {#if $connectionStatus === "connecting"}
    <div class="grid gap-3 sm:grid-cols-1 md:grid-cols-2" data-testid="llm-skeleton">
      {#each { length: SKELETON_COUNT } as _}
        <div class="space-y-3 rounded-xl glass-card glass-border p-4">
          <div class="flex items-center justify-between">
            <Skeleton variant="text" class="h-5 w-[50%]" />
            <Skeleton variant="text" class="h-5 w-14 rounded-full" />
          </div>
          <Skeleton variant="text" class="h-3 w-[70%]" />
          <Skeleton variant="text" class="h-3 w-[40%]" />
        </div>
      {/each}
    </div>
  {:else if $llmBackends.length === 0}
    <EmptyState
      icon={Brain}
      title={isOperator ? $t('llm.empty_operator') : $t('llm.empty_title')}
      subtitle={isOperator ? $t('llm.empty_operator_hint') : $t('llm.empty_subtitle')}
      ctaLabel={$t('llm.open_settings')}
      ctaAction={() => currentRoute.set("settings")}
      page="llm"
    />
  {:else}
    <div class="grid gap-3 sm:grid-cols-1 md:grid-cols-2" data-testid="llm-grid">
      {#each $llmBackends as backend (backend.name)}
        <div animate:flip={{ duration: 300 }} in:fly={{ y: 10, duration: 200 }}>
          <LlmBackendCard {backend} />
        </div>
      {/each}
    </div>

    <!-- Session statistics -->
    <div class="mt-6">
      <h2 class="mb-3 text-sm font-medium uppercase tracking-wider text-muted-foreground">
        {$t('llm.session_stats_title')}
      </h2>
      <LlmStats />
    </div>
  {/if}
</div>
