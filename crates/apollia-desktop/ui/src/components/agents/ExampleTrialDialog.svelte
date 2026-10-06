<script lang="ts">
  /**
   * ExampleTrialDialog - "Try with the example": copies the demonstration
   * dataset of an example into a folder the user chooses, then proposes what
   * it copied.
   *
   * It opens on the choice of the folder: what will be copied, that all of it
   * is made up, and that nothing is copied before the confirmation. Once
   * copied, it says where the dataset is and proposes the two things it is
   * for: the folder of reference documents as a source of the agent, checked
   * by default, and the workbook as a batch. "Launch the batch" hands the
   * workbook to the route, which opens the launch dialog on it. The decisions
   * live in `exampleTrial.ts`; this component holds the form and the calls.
   */
  import { t } from "svelte-i18n";
  import { open as openFolderDialog } from "@tauri-apps/plugin-dialog";
  import { CheckCircle2, FolderOpen } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { Dialog, DialogFooter } from "$lib/components/ui/dialog";
  import { Input } from "$lib/components/ui/input";
  import { getAgentDefinition } from "$lib/ipc/agentSheet";
  import { setAgentOverride } from "$lib/ipc/agentAdjust";
  import { addFolderConnection, listConnections } from "$lib/ipc/connections";
  import {
    copyExampleDataset,
    proposedDatasetFolder,
    type ExampleDatasetCopy,
  } from "$lib/ipc/exampleDataset";
  import { addSource, listSources } from "$lib/ipc/sources";
  import { addFolderAsSource, copyRefusal, type Trial, type TrialStage } from "./exampleTrial";

  interface Props {
    /** The trial the dialog is open on; `null` when it is closed. */
    trial: Trial | null;
    onclose: () => void;
    /** Open the launch of a batch of `agent` on the workbook at `workbook`. */
    onLaunchBatch: (agent: string, workbook: string) => void;
  }

  let { trial, onclose, onLaunchBatch }: Props = $props();

  let destination = $state("");
  let copying = $state(false);
  let copied = $state<ExampleDatasetCopy | null>(null);
  let alsoSource = $state(true);
  let finishing = $state(false);
  let error = $state<string | null>(null);

  const stage = $derived<TrialStage>(copied === null ? "choose" : "copied");
  const refusal = $derived(copyRefusal(destination, copying));

  function message(err: unknown): string {
    return err instanceof Error ? err.message : String(err);
  }

  // A fresh dialog at every opening, on the folder proposed for the copy.
  $effect(() => {
    if (trial === null) return;
    destination = "";
    copying = false;
    copied = null;
    alsoSource = true;
    finishing = false;
    error = null;
    let live = true;
    void proposedDatasetFolder($t("agents.trial.folder_name")).then((proposed) => {
      if (live && proposed !== null && destination === "") destination = proposed;
    });
    return () => {
      live = false;
    };
  });

  async function chooseFolder(): Promise<void> {
    try {
      const picked = await openFolderDialog({
        directory: true,
        multiple: false,
        title: $t("agents.trial.choose_title"),
      });
      if (typeof picked === "string") destination = picked;
    } catch (err) {
      error = message(err);
    }
  }

  async function copy(): Promise<void> {
    if (trial === null || refusal !== null) return;
    copying = true;
    error = null;
    try {
      copied = await copyExampleDataset(trial.example, destination.trim());
    } catch (err) {
      error = message(err);
    } finally {
      copying = false;
    }
  }

  // The source first, when it is checked: the batch then runs on an agent
  // that already consults the reference documents.
  async function launch(): Promise<void> {
    if (trial === null || copied === null) return;
    const { agent } = trial;
    const { folder, workbook } = copied;
    finishing = true;
    error = null;
    try {
      if (alsoSource) {
        await addFolderAsSource(
          { listConnections, addFolderConnection, listSources, addSource, getAgentDefinition, setAgentOverride },
          agent,
          folder,
        );
      }
      onLaunchBatch(agent, workbook);
    } catch (err) {
      error = message(err);
    } finally {
      finishing = false;
    }
  }
</script>

<Dialog
  open={trial !== null}
  {onclose}
  size="md"
  label={$t("agents.trial.title")}
  data-testid="example-trial-dialog"
>
  <div class="flex flex-col gap-4" data-stage={stage}>
    {#if copied === null}
      <header>
        <h2 class="m-0 text-base font-semibold text-foreground" data-testid="example-trial-title">
          {$t("agents.trial.title")}
        </h2>
      </header>
      <p class="m-0 text-body-sm text-foreground">{$t("agents.trial.what")}</p>
      <p class="m-0 text-caption text-muted-foreground" data-testid="example-trial-fictional">
        {$t("agents.trial.fictional")}
      </p>
      <div class="flex flex-col gap-1.5">
        <label class="text-caption font-medium text-foreground" for="example-trial-destination">
          {$t("agents.trial.destination")}
        </label>
        <div class="flex items-center gap-2">
          <Input
            id="example-trial-destination"
            class="min-w-0 flex-1"
            bind:value={destination}
            disabled={copying}
            data-testid="example-trial-destination"
          />
          <Button
            variant="outline"
            size="sm"
            class="shrink-0"
            disabled={copying}
            onclick={chooseFolder}
            data-testid="example-trial-choose"
          >
            {#snippet icon()}<FolderOpen size={12} />{/snippet}
            {$t("agents.trial.choose")}
          </Button>
        </div>
      </div>
    {:else}
      <header class="flex items-center gap-2.5">
        <CheckCircle2 size={18} class="shrink-0 text-success" />
        <h2 class="m-0 text-base font-semibold text-foreground" data-testid="example-trial-copied">
          {$t("agents.trial.copied_title")}
        </h2>
      </header>
      <p class="m-0 break-words text-body-sm text-foreground" data-testid="example-trial-where">
        {$t("agents.trial.copied_where", { values: { folder: copied.destination } })}
      </p>
      {#if copied.kept.length > 0}
        <p class="m-0 text-caption text-muted-foreground" data-testid="example-trial-kept">
          {$t("agents.trial.kept", { values: { count: copied.kept.length } })}
        </p>
      {/if}
      <p class="m-0 text-body-sm text-foreground">
        {$t("agents.trial.proposal", { values: { count: copied.rows } })}
      </p>
      <label class="flex cursor-pointer select-none items-center gap-2 text-body-sm text-foreground">
        <Checkbox bind:checked={alsoSource} disabled={finishing} data-testid="example-trial-also-source" />
        {$t("agents.trial.also_source")}
      </label>
    {/if}

    {#if error !== null}
      <p class="m-0 break-words text-caption text-destructive" role="alert" data-testid="example-trial-error">
        {error}
      </p>
    {/if}
  </div>

  <DialogFooter>
    {#if copied === null}
      <Button variant="outline" size="sm" disabled={copying} onclick={onclose} data-testid="example-trial-cancel">
        {$t("common.cancel")}
      </Button>
      <Button
        variant="primary-solid"
        size="sm"
        disabled={refusal !== null}
        onclick={copy}
        data-testid="example-trial-copy"
      >
        {$t("agents.trial.copy")}
      </Button>
    {:else}
      <Button variant="ghost" size="sm" disabled={finishing} onclick={onclose} data-testid="example-trial-later">
        {$t("agents.trial.later")}
      </Button>
      <Button
        variant="primary-solid"
        size="sm"
        disabled={finishing}
        onclick={launch}
        data-testid="example-trial-launch"
      >
        {$t("agents.trial.launch")}
      </Button>
    {/if}
  </DialogFooter>
</Dialog>
