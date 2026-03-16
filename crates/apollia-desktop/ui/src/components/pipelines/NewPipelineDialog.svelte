<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { PipelineInfo, RunPipelineResult } from "$lib/types";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    open: boolean;
    onclose: () => void;
    onrun: (runId: string, pipelineId: string) => void;
  }

  let { open, onclose, onrun }: Props = $props();

  let pipelines = $state<PipelineInfo[]>([]);
  let selectedPipelineId = $state("");
  let inputJson = $state("");
  let submitting = $state(false);
  let submitError = $state<string | null>(null);
  let jsonError = $state<string | null>(null);

  async function loadPipelines(): Promise<void> {
    try {
      pipelines = await invoke("list_pipelines");
    } catch {
      pipelines = [];
    }
  }

  function validateJson(value: string): boolean {
    if (!value.trim()) return true;
    try {
      JSON.parse(value);
      jsonError = null;
      return true;
    } catch {
      jsonError = $t('pipelines.invalid_json');
      return false;
    }
  }

  async function handleSubmit(): Promise<void> {
    if (!selectedPipelineId) return;
    if (!validateJson(inputJson)) return;

    submitting = true;
    submitError = null;
    try {
      const input = inputJson.trim() || null;
      const result: RunPipelineResult = await invoke("run_pipeline", {
        pipelineId: selectedPipelineId,
        input,
      });
      onrun(result.run_id, result.pipeline_id);
      onclose();
    } catch (err: unknown) {
      submitError = err instanceof Error ? err.message : String(err);
    } finally {
      submitting = false;
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      onclose();
    }
  }

  $effect(() => {
    if (open) {
      void loadPipelines();
      selectedPipelineId = "";
      inputJson = "";
      submitError = null;
      jsonError = null;
    }
  });
</script>

{#if open}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
    role="button"
    tabindex="-1"
    onclick={onclose}
    onkeydown={handleKeydown}
  >
    <div
      class="w-[480px] rounded-lg border bg-background p-6 shadow-lg"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={handleKeydown}
    >
      <h3 class="mb-4 text-lg font-semibold">{$t('pipelines.run_pipeline')}</h3>

      <div class="space-y-4">
        <div>
          <label class="mb-1 block text-sm font-medium" for="pipeline-select">{$t('pipelines.pipeline')}</label>
          {#if pipelines.length === 0}
            <p class="text-sm text-muted-foreground">
              {$t('pipelines.no_pipelines')}
            </p>
          {:else}
            <select
              id="pipeline-select"
              class="w-full rounded-md border bg-background px-3 py-2 text-sm"
              bind:value={selectedPipelineId}
            >
              <option value="" disabled>{$t('pipelines.select_pipeline')}</option>
              {#each pipelines as pipeline}
                <option value={pipeline.id}>
                  {pipeline.id}{pipeline.description ? ` — ${pipeline.description}` : ""}
                </option>
              {/each}
            </select>
          {/if}
        </div>

        <div>
          <label class="mb-1 block text-sm font-medium" for="pipeline-input">
            {$t('pipelines.input_json')} <span class="font-normal text-muted-foreground">{$t('pipelines.input_json_optional')}</span>
          </label>
          <textarea
            id="pipeline-input"
            class="w-full rounded-md border bg-background px-3 py-2 font-mono text-sm {jsonError
              ? 'border-[hsl(var(--destructive))]'
              : ''}"
            rows="5"
            placeholder={'{"key": "value"}'}
            bind:value={inputJson}
            oninput={() => validateJson(inputJson)}
          ></textarea>
          {#if jsonError}
            <p class="mt-1 text-xs text-[hsl(var(--destructive))]">{jsonError}</p>
          {/if}
        </div>

        {#if submitError}
          <p class="text-sm text-[hsl(var(--destructive))]">{submitError}</p>
        {/if}

        <div class="flex justify-end gap-2">
          <Button variant="outline" size="sm" onclick={onclose}>{$t('common.cancel')}</Button>
          <Button
            size="sm"
            onclick={handleSubmit}
            disabled={!selectedPipelineId || submitting || !!jsonError}
          >
            {submitting ? $t('pipelines.launching') : $t('pipelines.launch')}
          </Button>
        </div>
      </div>
    </div>
  </div>
{/if}
