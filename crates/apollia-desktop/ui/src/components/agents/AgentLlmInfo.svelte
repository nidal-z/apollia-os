<script lang="ts">
  import { t } from "svelte-i18n";
  import { llmBackends } from "$lib/stores/llm";
  import { uiMode } from "$lib/stores/mode";
  import { Badge } from "$lib/components/ui/badge";
  import { Cpu, Cloud } from "lucide-svelte";

  /** Produce a human-friendly description of the LLM backend for operator mode. */
  function humanizeBackend(backendType: string, model: string): string {
    const displayModel = prettifyModelName(model);
    if (backendType === "embedded") {
      return `${$t('agent_detail.llm_local')} (${displayModel}) - ${$t('agent_detail.llm_free')}`;
    }
    return `${$t('agent_detail.llm_cloud')} (${displayModel})`;
  }

  /** Convert a raw model identifier into a prettier display name. */
  function prettifyModelName(model: string): string {
    const lower = model.toLowerCase();
    if (lower.includes("mistral-7b") || lower.includes("mistral7b")) return "Mistral 7B";
    if (lower.includes("mistral")) return "Mistral";
    if (lower.includes("llama-3") || lower.includes("llama3")) return "Llama 3";
    if (lower.includes("llama")) return "Llama";
    if (lower.includes("phi-3") || lower.includes("phi3")) return "Phi-3";
    if (lower.includes("claude")) return model;
    if (lower.includes("gpt-4o-mini")) return "GPT-4o Mini";
    if (lower.includes("gpt-4o")) return "GPT-4o";
    if (lower.includes("gpt-4")) return "GPT-4";
    if (lower.includes("gpt-3.5")) return "GPT-3.5";
    return model;
  }

  let firstBackend = $derived($llmBackends[0] ?? null);
</script>

<section data-testid="agent-llm-section">
  <h3 class="mb-3 text-sm font-medium">{$t('agent_detail.llm_title')}</h3>

  {#if !firstBackend}
    <p class="text-xs text-muted-foreground">{$t('agent_detail.no_llm')}</p>
  {:else}
    <div class="flex items-start gap-3 rounded-md border px-3 py-2">
      {#if firstBackend.provider === "llama-cpp"}
        <Cpu size={16} class="mt-0.5 shrink-0 text-muted-foreground" />
      {:else}
        <Cloud size={16} class="mt-0.5 shrink-0 text-muted-foreground" />
      {/if}

      <div class="min-w-0 flex-1">
        <span class="text-sm">{humanizeBackend(firstBackend.provider === "llama-cpp" ? "embedded" : "api", firstBackend.model)}</span>

        {#if $uiMode === "builder"}
          <div class="mt-1 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
            <span>{$t('agent_detail.llm_backend')}: {firstBackend.name}</span>
            <span>{$t('agent_detail.llm_model')}: {firstBackend.model}</span>
            <Badge
              variant={firstBackend.enabled ? "success" : "destructive"}
              class="text-[10px]"
            >
              {firstBackend.enabled ? "ENABLED" : "DISABLED"}
            </Badge>
          </div>
        {/if}
      </div>
    </div>

    {#if $uiMode === "builder" && $llmBackends.length > 1}
      <p class="mt-2 text-xs text-muted-foreground">
        +{$llmBackends.length - 1} {$t('agent_detail.llm_more_backends')}
      </p>
    {/if}
  {/if}
</section>
