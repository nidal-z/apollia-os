<script lang="ts">
  /**
   * BatchRequestCard - what the batch was asked at launch, kept in sight
   * while its results are read: the instruction given for each item, the
   * fields of the answer with what each holds, and the synthesis asked for
   * the end. Beside it, the files the batch wrote, each opened or shown in
   * its folder in one click.
   */
  import { t } from "svelte-i18n";
  import { FileText, FolderOpen } from "lucide-svelte";
  import { Card } from "$lib/components/operator";
  import { Button } from "$lib/components/ui/button";
  import { addToast } from "$lib/components/ui/toast/store";
  import type { BatchTask } from "$lib/ipc/batches";
  import { openFile, revealPath } from "$lib/utils/fileRef";
  import { fileNameOf } from "../launchBatch";

  interface Props {
    task: BatchTask | null;
    /** The files the batch wrote, with what each is. */
    files: { kind: "results" | "synthesis" | "export"; path: string }[];
  }

  let { task, files }: Props = $props();

  const KIND_KEYS = {
    results: "batch_view.files.kind_results",
    synthesis: "batch_view.files.kind_synthesis",
    export: "batch_view.files.kind_export",
  } as const;

  const outputs = $derived((task?.outputs ?? []).filter((output) => output.name.trim() !== ""));
  const instruction = $derived(task?.instruction.trim() ?? "");
  const synthesis = $derived(task?.synthesis?.trim() ?? "");

  async function run(action: () => Promise<void>): Promise<void> {
    try {
      await action();
    } catch (err) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    }
  }
</script>

<Card class="grid gap-4 p-4 lg:grid-cols-[minmax(0,1fr)_minmax(16rem,22rem)]" data-testid="batch-request">
  <div class="min-w-0 space-y-2">
    <h3 class="m-0 text-caption font-medium uppercase tracking-wide text-muted-foreground">
      {$t("batch_view.request.title")}
    </h3>
    {#if instruction !== ""}
      <p class="m-0 whitespace-pre-wrap break-words text-body-sm text-foreground" data-testid="batch-request-instruction">
        {instruction}
      </p>
    {:else}
      <p class="m-0 text-body-sm text-muted-foreground">{$t("batch_view.request.no_instruction")}</p>
    {/if}
    {#if outputs.length > 0}
      <details class="text-body-sm" data-testid="batch-request-outputs">
        <summary class="cursor-pointer text-caption text-muted-foreground">
          {$t("batch_view.request.outputs", { values: { count: outputs.length, names: outputs.map((o) => o.name).join(", ") } })}
        </summary>
        <dl class="m-0 mt-2 grid grid-cols-1 gap-x-4 gap-y-1.5 sm:grid-cols-[minmax(7rem,auto)_1fr]">
          {#each outputs as output (output.name)}
            <dt class="text-caption font-medium text-foreground">{output.name}</dt>
            <dd class="m-0 break-words text-caption text-muted-foreground">{output.description || "·"}</dd>
          {/each}
        </dl>
      </details>
    {/if}
    {#if synthesis !== ""}
      <p class="m-0 break-words text-caption text-muted-foreground" data-testid="batch-request-synthesis">
        {$t("batch_view.request.synthesis", { values: { text: synthesis } })}
      </p>
    {/if}
  </div>

  <div class="min-w-0 space-y-2" data-testid="batch-files">
    <h3 class="m-0 text-caption font-medium uppercase tracking-wide text-muted-foreground">
      {$t("batch_view.files.title")}
    </h3>
    {#if files.length === 0}
      <p class="m-0 text-caption text-muted-foreground">{$t("batch_view.files.none")}</p>
    {:else}
      <ul class="m-0 list-none space-y-2 p-0">
        {#each files as file (file.path)}
          <li class="space-y-1" data-testid="batch-file" data-kind={file.kind}>
            <span class="flex min-w-0 items-center gap-1.5 text-body-sm text-foreground">
              <FileText size={13} class="shrink-0 text-muted-foreground" />
              <span class="truncate" title={file.path}>{fileNameOf(file.path)}</span>
            </span>
            <span class="block text-caption text-muted-foreground">{$t(KIND_KEYS[file.kind])}</span>
            <span class="flex gap-1.5">
              <Button variant="outline" size="sm" onclick={() => void run(() => openFile(file.path))} data-testid="batch-file-open">
                {$t("batch_view.files.open")}
              </Button>
              <Button variant="ghost" size="sm" onclick={() => void run(() => revealPath(file.path))} data-testid="batch-file-reveal">
                {#snippet icon()}<FolderOpen size={12} />{/snippet}
                {$t("batch_view.files.reveal")}
              </Button>
            </span>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</Card>
