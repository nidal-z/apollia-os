<script lang="ts">
  /**
   * AccountOneDriveChoices - the OneDrive folders and files a connected
   * Microsoft account lets Apollia read, under the account on its connection's
   * page: listed, removed, or chosen by browsing the drive.
   */
  import { t } from "svelte-i18n";
  import { File, Folder } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { reportError } from "$lib/errors/reportError";
  import {
    onedriveBrowse,
    onedriveChoose,
    onedriveForget,
    onedriveListChoices,
    type OneDriveChoice,
    type OneDriveEntry,
  } from "$lib/ipc/onedrive";
  import { currentFolder, goUp, openFolder, type TrailStep } from "$lib/connections/onedriveTrail";

  interface Props {
    accountId: string;
    /** Choices already known, shown before the first read answers. */
    initialChoices?: OneDriveChoice[];
  }

  let { accountId, initialChoices = [] }: Props = $props();

  // svelte-ignore state_referenced_locally
  let choices = $state<OneDriveChoice[]>(initialChoices);
  let browsing = $state(false);
  let trail = $state<TrailStep[]>([]);
  let entries = $state<OneDriveEntry[]>([]);
  let busy = $state(false);

  const ERROR_TESTID = "onedrive-choices-error-toast";

  async function refreshChoices(): Promise<void> {
    try {
      choices = await onedriveListChoices(accountId);
    } catch (err) {
      reportError(err, { surface: "toast", testid: ERROR_TESTID });
    }
  }

  $effect(() => {
    if (accountId) void refreshChoices();
  });

  async function show(next: TrailStep[]): Promise<void> {
    busy = true;
    try {
      entries = await onedriveBrowse(accountId, currentFolder(next));
      trail = next;
      browsing = true;
    } catch (err) {
      reportError(err, { surface: "toast", testid: ERROR_TESTID });
    } finally {
      busy = false;
    }
  }

  async function choose(entry: OneDriveEntry): Promise<void> {
    busy = true;
    try {
      choices = await onedriveChoose(accountId, entry.id);
      entries = entries.map((e) => (e.id === entry.id ? { ...e, chosen: true } : e));
    } catch (err) {
      reportError(err, { surface: "toast", testid: ERROR_TESTID });
    } finally {
      busy = false;
    }
  }

  async function forget(choice: OneDriveChoice): Promise<void> {
    busy = true;
    try {
      choices = await onedriveForget(accountId, choice.id);
      entries = entries.map((e) => (e.id === choice.id ? { ...e, chosen: false } : e));
    } catch (err) {
      reportError(err, { surface: "toast", testid: ERROR_TESTID });
    } finally {
      busy = false;
    }
  }
</script>

<div class="mt-2 text-caption text-muted-foreground" data-testid="onedrive-choices-{accountId}">
  <div class="font-medium text-foreground">{$t("connections.onedrive_choices.title")}</div>
  <p class="mt-0.5">{$t("connections.onedrive_choices.intro")}</p>

  {#if choices.length === 0}
    <p class="mt-1">{$t("connections.onedrive_choices.none")}</p>
  {:else}
    <ul class="mt-1">
      {#each choices as choice (choice.id)}
        <li class="flex items-center gap-2" data-testid="onedrive-choice-{choice.id}">
          {#if choice.is_folder}<Folder size={12} aria-hidden="true" />{:else}<File size={12} aria-hidden="true" />{/if}
          <span class="min-w-0 flex-1 truncate" title={choice.path}>{choice.name}</span>
          <Button
            variant="ghost"
            size="sm"
            onclick={() => forget(choice)}
            disabled={busy}
            data-testid="onedrive-forget-{choice.id}"
          >
            {$t("connections.onedrive_choices.forget")}
          </Button>
        </li>
      {/each}
    </ul>
  {/if}

  {#if browsing}
    <div class="mt-2 border-t border-border/40 pt-2">
      <div class="flex items-center gap-2">
        <span class="min-w-0 flex-1 truncate">
          {[$t("connections.onedrive_choices.root"), ...trail.map((s) => s.name)].join(" / ")}
        </span>
        <Button
          variant="ghost"
          size="sm"
          onclick={() => show(goUp(trail))}
          disabled={busy || trail.length === 0}
          data-testid="onedrive-up-{accountId}"
        >
          {$t("connections.onedrive_choices.up")}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          onclick={() => (browsing = false)}
          data-testid="onedrive-browse-close-{accountId}"
        >
          {$t("connections.onedrive_choices.done")}
        </Button>
      </div>
      {#if entries.length === 0}
        <p class="mt-1">{$t("connections.onedrive_choices.empty_folder")}</p>
      {/if}
      <ul class="mt-1">
        {#each entries as entry (entry.id)}
          <li class="flex items-center gap-2" data-testid="onedrive-entry-{entry.id}">
            {#if entry.is_folder}<Folder size={12} aria-hidden="true" />{:else}<File size={12} aria-hidden="true" />{/if}
            <span class="min-w-0 flex-1 truncate">{entry.name}</span>
            {#if entry.is_folder}
              <Button
                variant="ghost"
                size="sm"
                onclick={() => show(openFolder(trail, entry))}
                disabled={busy}
                data-testid="onedrive-open-{entry.id}"
              >
                {$t("connections.onedrive_choices.open")}
              </Button>
            {/if}
            <Button
              variant="ghost"
              size="sm"
              onclick={() => choose(entry)}
              disabled={busy || entry.chosen}
              data-testid="onedrive-choose-{entry.id}"
            >
              {entry.chosen ? $t("connections.onedrive_choices.chosen") : $t("connections.onedrive_choices.choose")}
            </Button>
          </li>
        {/each}
      </ul>
    </div>
  {:else}
    <Button
      variant="ghost"
      size="sm"
      class="mt-1"
      onclick={() => show([])}
      disabled={busy}
      data-testid="onedrive-browse-{accountId}"
    >
      {$t("connections.onedrive_choices.browse")}
    </Button>
  {/if}
</div>
