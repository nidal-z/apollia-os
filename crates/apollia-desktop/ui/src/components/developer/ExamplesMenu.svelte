<script lang="ts">
  /**
   * The "Examples" menu of a card of the local server: the calls a developer
   * copies to reach the model, the agent or the workflow of the card. Each
   * entry has its copy button; choosing one shows its text under the list,
   * with the choice of the SDK for Python and JavaScript. An entry that
   * cannot be used on this card stays, greyed, with its reason on hover.
   *
   * The examples are written for the address the server reports when the
   * menu opens, and name the token by its variable when a call presents one.
   */
  import { t } from "svelte-i18n";
  import { Code, Copy } from "lucide-svelte";
  import { ActionMenu } from "$lib/components/ui/action-menu";
  import { Button } from "$lib/components/ui/button";
  import { Tooltip } from "$lib/components/ui/tooltip";
  import { addToast } from "$lib/components/ui/toast";
  import { FilterChipBar } from "$lib/components/operator";
  import { developerMode } from "$lib/stores/developerMode";
  import { serverSettings } from "$lib/stores/serverSettings";
  import { getLocalServer, type LocalServerView } from "$lib/ipc/localServer";
  import { copyIdentifier } from "./servedItems";
  import {
    FIRST_EXAMPLE,
    SDK_LABELS,
    chosenSdk,
    exampleEntries,
    exampleFor,
    exampleSdks,
    exampleServer,
    exampleSlug,
    type ExampleId,
    type ExampleSdk,
    type ExampleTarget,
    type ExampleTexts,
  } from "./examples";

  interface Props {
    /** What the examples call. */
    target: ExampleTarget;
  }

  let { target }: Props = $props();

  let open = $state(false);
  let view = $state<LocalServerView | null>(null);
  let shown = $state<ExampleId>(FIRST_EXAMPLE);
  let sdk = $state<ExampleSdk>("openai");

  // The address and the network follow the server: read them at each opening.
  $effect(() => {
    if (!open) return;
    let live = true;
    void getLocalServer()
      .then((read) => {
        if (live) view = read;
      })
      .catch(() => {
        // The state is unreadable: the examples keep the configured address.
      });
    return () => {
      live = false;
    };
  });

  const server = $derived(exampleServer(view, $serverSettings));
  const entries = $derived(exampleEntries(target, server));
  const texts = $derived<ExampleTexts>({
    prompt: $t("dev.developer.examples.prompt"),
    toolPrompt: $t("dev.developer.examples.prompt_tool"),
    mcpPrompt: $t("dev.developer.examples.prompt_mcp"),
  });
  // An entry greyed out since it was chosen gives the preview back to the first.
  const current = $derived(
    entries.find((entry) => entry.id === shown && entry.reason === null)?.id ?? FIRST_EXAMPLE,
  );
  const sdks = $derived(exampleSdks(current, target));
  const preview = $derived(exampleFor(current, target, server, sdk, texts) ?? "");
  const currentLabel = $derived($t(entries.find((entry) => entry.id === current)?.label ?? ""));

  async function copy(id: ExampleId): Promise<void> {
    const text = exampleFor(id, target, server, sdk, texts);
    const done =
      text !== null && (await copyIdentifier(text, (value) => navigator.clipboard.writeText(value)));
    addToast(
      $t(done ? "dev.developer.examples.copied" : "dev.developer.examples.copy_failed"),
      done ? "success" : "error",
    );
  }
</script>

{#if $developerMode}
  <ActionMenu bind:open align="start" class="w-[32rem] max-w-[90vw] p-2">
    {#snippet triggerSlot(props)}
      <Button {...props} variant="outline" size="sm" data-testid="developer-examples-open">
        {#snippet icon()}<Code size={14} aria-hidden="true" />{/snippet}
        {$t("dev.developer.examples.open")}
      </Button>
    {/snippet}
    {#snippet body()}
      <div class="space-y-2" data-testid="developer-examples-menu">
        <p class="truncate px-2 text-xs text-muted-foreground" data-testid="developer-examples-title">
          {$t("dev.developer.examples.title", { values: { identifier: target.identifier } })}
        </p>
        <ul class="flex flex-col gap-0.5" role="menu">
          {#each entries as entry (entry.id)}
            <li role="none" class="flex items-center gap-1">
              {#if entry.reason}
                <Tooltip content={$t(entry.reason)} side="right">
                  <span
                    role="menuitem"
                    aria-disabled="true"
                    tabindex="-1"
                    class="flex w-full cursor-not-allowed items-center rounded-sm px-2 py-1.5 text-left text-body-xs text-foreground opacity-50"
                    data-testid={`developer-example-entry-${exampleSlug(entry.id)}`}
                  >
                    {$t(entry.label)}
                  </span>
                </Tooltip>
              {:else}
                <button
                  type="button"
                  role="menuitem"
                  aria-current={current === entry.id}
                  class="flex min-w-0 flex-1 items-center rounded-sm px-2 py-1.5 text-left text-body-xs text-foreground transition-colors hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 aria-[current=true]:bg-muted"
                  onclick={() => (shown = entry.id)}
                  data-testid={`developer-example-entry-${exampleSlug(entry.id)}`}
                >
                  {$t(entry.label)}
                </button>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label={$t("dev.developer.examples.copy_entry", {
                    values: { name: $t(entry.label) },
                  })}
                  onclick={() => void copy(entry.id)}
                  data-testid={`developer-example-copy-${exampleSlug(entry.id)}`}
                >
                  <Copy size={14} aria-hidden="true" />
                </Button>
              {/if}
            </li>
          {/each}
        </ul>

        <div class="space-y-2 rounded-md bg-surface-2 p-2">
          <div class="flex flex-wrap items-center justify-between gap-2">
            <p class="text-xs text-muted-foreground" data-testid="developer-example-preview-title">
              {$t("dev.developer.examples.preview", { values: { name: currentLabel } })}
            </p>
            <div class="flex items-center gap-2">
              {#if sdks.length > 1}
                <FilterChipBar
                  size="compact"
                  chips={sdks.map((key) => ({
                    key,
                    label: $t(SDK_LABELS[key]),
                  }))}
                  activeKey={chosenSdk(current, target, sdk)}
                  onchange={(key) => (sdk = key as ExampleSdk)}
                  aria-label={$t("dev.developer.examples.sdk_choice")}
                  testidPrefix="developer-example-sdk"
                />
              {/if}
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={$t("dev.developer.examples.copy_entry", { values: { name: currentLabel } })}
                onclick={() => void copy(current)}
                data-testid="developer-example-preview-copy"
              >
                <Copy size={14} aria-hidden="true" />
              </Button>
            </div>
          </div>
          <pre
            class="max-h-72 overflow-auto whitespace-pre font-mono text-micro text-foreground"
            data-testid="developer-example-preview">{preview}</pre>
          {#if current === "curl_mcp"}
            <p class="text-xs text-muted-foreground" data-testid="developer-example-mcp-note">
              {$t("dev.developer.examples.mcp_note")}
            </p>
          {/if}
        </div>
      </div>
    {/snippet}
  </ActionMenu>
{/if}
