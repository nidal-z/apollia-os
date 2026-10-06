<script lang="ts">
  /**
   * TryConnectionDialog - "Try" a connection: a conversation with Apollia
   * limited to the tools of that connection. The operator picks one of the
   * questions offered (one per tool example, or the general question when the
   * connection carries none) or types their own, then launches the trial.
   * Presentation only; the parent builds the questions and runs the trial.
   */
  import { t } from "svelte-i18n";
  import { Dialog } from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { RadioGroup, RadioItem } from "$lib/components/ui/radio";
  import { Spinner } from "$lib/components/ui/progress";
  import type { TryQuestion } from "$lib/connections/tryConnectorPrompt";

  interface Props {
    open: boolean;
    /** Name of the connection, as the screen shows it. */
    name: string;
    questions: TryQuestion[];
    launching?: boolean;
    onclose: () => void;
    onlaunch: (prompt: string) => void;
  }

  let { open, name, questions, launching = false, onclose, onlaunch }: Props = $props();

  let selected = $state("");
  let own = $state("");

  // A new connection opens on its first question and an empty field.
  $effect(() => {
    if (open) {
      selected = questions[0]?.id ?? "";
      own = "";
    }
  });

  const prompt = $derived(
    own.trim() || questions.find((q) => q.id === selected)?.question || "",
  );
  const onlyGeneral = $derived(questions.length === 1 && questions[0]?.general === true);

  function launch(): void {
    if (prompt.length > 0 && !launching) onlaunch(prompt);
  }
</script>

<Dialog
  {open}
  {onclose}
  size="md"
  title={$t("connections.try.title", { values: { name } })}
  data-testid="try-connection-dialog"
>
  <div class="space-y-4">
    <p class="text-caption text-muted-foreground">{$t("connections.try.subtitle")}</p>

    <div class="space-y-2">
      <p class="text-body-sm font-medium text-foreground">{$t("connections.try.choose")}</p>
      <RadioGroup value={selected} data-testid="try-connection-questions">
        {#each questions as q (q.id)}
          <RadioItem
            value={q.id}
            checked={selected === q.id && own.trim().length === 0}
            onchange={(v) => {
              selected = v;
              own = "";
            }}
            class="items-start rounded-md border border-border px-3 py-2.5"
            data-testid="try-connection-question-{q.id}"
          >
            <span class="block text-body-sm text-foreground">
              {q.general ? $t("connections.try.general_label") : `${name} · ${q.label}`}
            </span>
            <span class="block text-caption text-muted-foreground">
              {$t("connections.try.quoted", { values: { question: q.question } })}
            </span>
          </RadioItem>
        {/each}
      </RadioGroup>
      {#if onlyGeneral}
        <p class="text-caption text-muted-foreground" data-testid="try-connection-no-example">
          {$t("connections.try.no_example")}
        </p>
      {/if}
    </div>

    <div class="space-y-2">
      <label for="try-connection-own" class="text-body-sm font-medium text-foreground">
        {$t("connections.try.own")}
      </label>
      <Input
        id="try-connection-own"
        bind:value={own}
        placeholder={$t("connections.try.own_placeholder")}
        data-testid="try-connection-own"
      />
    </div>
  </div>

  <div class="mt-6 flex items-center justify-end gap-2">
    <Button variant="outline" size="sm" onclick={onclose} data-testid="try-connection-cancel">
      {$t("common.cancel")}
    </Button>
    <Button
      variant="primary-solid"
      size="sm"
      onclick={launch}
      disabled={prompt.length === 0 || launching}
      data-testid="try-connection-launch"
    >
      {#snippet icon()}
        {#if launching}<Spinner size={12} />{/if}
      {/snippet}
      {$t("connections.try.launch")}
    </Button>
  </div>
</Dialog>
