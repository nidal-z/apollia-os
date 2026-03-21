<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { PipelineInfo, RunPipelineResult } from "$lib/types";
  import { Button } from "$lib/components/ui/button";
  import { Select } from "$lib/components/ui/select";
  import { Textarea } from "$lib/components/ui/textarea";
  import { Dialog } from "$lib/components/ui/dialog";

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

<Dialog open={open} onclose={onclose} size="md" title={$t('pipelines.run_pipeline')}>
  <div class="space-y-4">
    <div>
      <label class="mb-1 block text-sm font-medium" for="pipeline-select">{$t('pipelines.pipeline')}</label>
      {#if pipelines.length === 0}
        <p class="text-sm text-muted-foreground">
          {$t('pipelines.no_pipelines')}
        </p>
      {:else}
        <Select
          id="pipeline-select"
          bind:value={selectedPipelineId}
        >
          <option value="" disabled>{$t('pipelines.select_pipeline')}</option>
          {#each pipelines as pipeline}
            <option value={pipeline.id}>
              {pipeline.id}{pipeline.description ? ` — ${pipeline.description}` : ""}
            </option>
          {/each}
        </Select>
      {/if}
    </div>

    <div>
      <label class="mb-1 block text-sm font-medium" for="pipeline-input">
        {$t('pipelines.input_json')} <span class="font-normal text-muted-foreground">{$t('pipelines.input_json_optional')}</span>
      </label>
      <Textarea
        id="pipeline-input"
        class="font-mono {jsonError ? 'border-destructive' : ''}"
        rows={5}
        placeholder={'{"key": "value"}'}
        bind:value={inputJson}
        oninput={() => validateJson(inputJson)}
      />
      {#if jsonError}
        <p class="mt-1 text-xs text-destructive">{jsonError}</p>
      {/if}
    </div>

    {#if submitError}
      <p class="text-sm text-destructive">{submitError}</p>
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
</Dialog>
