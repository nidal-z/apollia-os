<script lang="ts">
  /**
   * AgentExamples - the examples the application proposes, under the lists of
   * the Agents screen.
   *
   * A band names them and opens them; opened, one card per example gives its
   * name, its mission and what it requires. "Install" hands the example to
   * the route, which opens the preview every package gets; an installed
   * example says so and leads to the sheet of its agent. An example that
   * comes with a demonstration dataset also offers "Try with the example",
   * installed or not. On a screen with nothing else the cards are open from
   * the start. Nothing is installed or copied from here without a gesture.
   */
  import { t } from "svelte-i18n";
  import { Sparkles } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import type { ExamplePackage } from "$lib/ipc/packageExamples";
  import { exampleCards, type ExampleCard } from "./agentExamples";
  import { hasDataset } from "./exampleTrial";

  interface Props {
    examples: ExamplePackage[];
    /** Open on the cards from the start: the screen has nothing else. */
    openByDefault: boolean;
    /** Open the preview of the example of that package name. */
    onInstall: (name: string) => void;
    /** Open the sheet of the agent an installed example brought. */
    onOpenSheet: (agentId: string) => void;
    /** Try the example of that card on its demonstration dataset. */
    onTry: (card: ExampleCard) => void;
  }

  let { examples, openByDefault, onInstall, onOpenSheet, onTry }: Props = $props();

  // What the user chose; until then the screen decides.
  let chosen = $state<boolean | null>(null);
  const open = $derived(chosen ?? openByDefault);
  const cards = $derived(exampleCards(examples));
  const kicker = $derived($t("agents.examples.kicker", { values: { count: cards.length } }));
</script>

{#if cards.length > 0}
  <section class="mt-6" data-testid="agents-examples" data-open={open} aria-label={kicker}>
    {#if open}
      <div class="mb-1.5 px-2 text-overline uppercase text-muted-foreground/80">{kicker}</div>
      <p class="mb-3 px-2 text-caption text-muted-foreground">{$t("agents.examples.intro")}</p>
      <ul class="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
        {#each cards as card (card.name)}
          <li
            class="flex flex-col gap-2 rounded-lg border border-border bg-surface-1 p-4"
            data-testid="agents-example-card"
            data-example={card.name}
            data-installed={card.installed}
          >
            <div class="flex items-center gap-2.5">
              <span
                class="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border border-border bg-surface-1 text-primary"
              >
                <Sparkles size={13} />
              </span>
              <h3 class="min-w-0 flex-1 truncate text-body-sm font-medium text-foreground">{card.title}</h3>
              {#if card.installed}
                <Badge size="sm" variant="success" class="shrink-0" data-testid="agents-example-installed">
                  {$t("agents.examples.installed")}
                </Badge>
              {/if}
            </div>
            <p class="line-clamp-4 text-caption text-muted-foreground" title={card.mission}>{card.mission}</p>
            {#if card.requires.length > 0}
              <p class="text-caption text-muted-foreground" data-testid="agents-example-requires">
                {$t("agents.examples.requires", { values: { list: card.requires.join(", ") } })}
              </p>
            {/if}
            <div class="mt-auto flex flex-col gap-2 pt-1">
              {#if hasDataset(card.name)}
                <Button
                  variant="outline"
                  size="sm"
                  class="w-full"
                  onclick={() => onTry(card)}
                  data-testid="agents-example-try"
                >
                  {$t("agents.trial.action")}
                </Button>
              {/if}
              {#if !card.installed}
                <Button
                  variant="primary-solid"
                  size="sm"
                  class="w-full"
                  onclick={() => onInstall(card.name)}
                  data-testid="agents-example-install"
                >
                  {$t("agents.examples.install")}
                </Button>
              {:else if card.sheetAgent !== null}
                {@const agentId = card.sheetAgent}
                <Button
                  variant="outline"
                  size="sm"
                  class="w-full"
                  onclick={() => onOpenSheet(agentId)}
                  data-testid="agents-example-open-sheet"
                >
                  {$t("agents.examples.open_sheet")}
                </Button>
              {/if}
            </div>
          </li>
        {/each}
      </ul>
    {:else}
      <div class="flex items-center gap-3 rounded-lg border border-border bg-surface-1 px-4 py-3">
        <Sparkles size={14} class="shrink-0 text-primary" />
        <div class="min-w-0 flex-1">
          <div class="text-overline uppercase text-muted-foreground/80">{kicker}</div>
          <p class="truncate text-caption text-muted-foreground" data-testid="agents-examples-names">
            {$t("agents.examples.band", { values: { names: cards.map((card) => card.title).join(", ") } })}
          </p>
        </div>
        <Button
          variant="outline"
          size="sm"
          class="shrink-0"
          onclick={() => (chosen = true)}
          data-testid="agents-examples-show"
        >
          {$t("agents.examples.show")}
        </Button>
      </div>
    {/if}
  </section>
{/if}
