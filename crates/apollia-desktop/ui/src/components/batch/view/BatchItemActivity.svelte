<script lang="ts">
  /**
   * BatchItemActivity - what the agent did, and is doing, on one item of a
   * batch, read as the conversation reads a turn: its plan and the state of
   * each step, each tool on the same foldable line as in the chat, what the
   * model wrote and is writing, the accesses it asked for and the plans it
   * had to redo.
   */
  import { locale, t } from "svelte-i18n";
  import { Check, CircleDashed, ListChecks, RotateCcw, ShieldQuestion, Sparkles, X } from "lucide-svelte";
  import { Spinner } from "$lib/components/ui/progress";
  import type { ReasoningItem } from "$lib/chat/reasoning";
  import { formatSecondsLocalized } from "$lib/chat/duration";
  import { stepActionKey } from "$lib/chat/toolStep";
  import { developerMode } from "$lib/stores/developerMode";
  import ReasoningBlock from "../../chat/ReasoningBlock.svelte";
  import ToolStep from "../../chat/ToolStep.svelte";
  import { splitThinking, type ActivityEntry, type ItemActivity } from "./itemActivity";

  interface Props {
    activity: ItemActivity;
    /** The item's run is going on: what is written now is shown as it comes. */
    running: boolean;
    sessionId: string | null;
    /** The item has run, or runs: an empty record then says so, not "not processed". */
    ran: boolean;
  }

  let { activity, running, sessionId, ran }: Props = $props();

  let open = $state<Record<number, boolean>>({});

  function toggle(index: number): void {
    open = { ...open, [index]: !open[index] };
  }

  /** A duration worth reading: nothing under a tenth of a second. */
  function seconds(ms: number | null): string {
    return ms !== null && ms >= 100 ? `${formatSecondsLocalized(ms, $locale ?? "en")} s` : "";
  }

  /**
   * What a step says: its own description, or, when the record named it by
   * its tool alone, the action of that tool in plain words with its target.
   */
  function stepText(desc: string, target: string | null): string {
    const key = stepActionKey(desc);
    if (key === null) return desc;
    return target === null ? $t(key) : `${$t(key)} · ${target}`;
  }

  type ToolEntry = Extract<ActivityEntry, { kind: "tool" }>;

  /** A tool entry as the chat's tool step reads it. */
  function asToolItem(entry: ToolEntry, index: number): Extract<ReasoningItem, { kind: "tool_call" }> {
    let args: Record<string, unknown> = {};
    if (entry.input !== null) {
      try {
        const parsed: unknown = JSON.parse(entry.input);
        if (typeof parsed === "object" && parsed !== null) args = parsed as Record<string, unknown>;
      } catch {
        // A preview cut short: the target read from it still names the call.
        if (entry.target !== null) args = { path: entry.target };
      }
    }
    return {
      id: `batch-item-tool-${index}`,
      kind: "tool_call",
      status: entry.status === "running" ? "running" : entry.status === "done" ? "success" : "error",
      tool: entry.tool,
      args,
      output: unwrapped(entry.output),
      duration_ms: entry.durationMs !== null && entry.durationMs >= 100 ? entry.durationMs : null,
    };
  }

  /**
   * The text a tool gave back. A planned step's tool answers its whole result
   * object (`{"content": "...", "total_lines": 25}`); the chat's tool bodies
   * read the text, as a conversation shows it.
   */
  function unwrapped(output: string | null): string | null {
    if (output === null) return null;
    try {
      const parsed: unknown = JSON.parse(output);
      if (typeof parsed === "object" && parsed !== null) {
        const content = (parsed as Record<string, unknown>).content;
        if (typeof content === "string") return content;
      }
    } catch {
      // Not an object: the output is already text.
    }
    return output;
  }

  const skin = $derived($developerMode ? "builder" : "operator");
  const now = $derived(splitThinking(activity.writing));
</script>

{#if activity.entries.length === 0 && activity.writing === ""}
  <p class="m-0 flex items-center gap-2 text-body-sm text-muted-foreground" data-testid="batch-item-activity-empty">
    {#if running}
      <Spinner size={12} />
      {$t("batch_view.activity.starting")}
    {:else}
      {$t(ran ? "batch_view.activity.none_recorded" : "batch_view.activity.none")}
    {/if}
  </p>
{:else}
  <ol class="m-0 list-none space-y-2 p-0" data-testid="batch-item-activity">
    {#each activity.entries as entry, index (index)}
      <li data-kind={entry.kind}>
        {#if entry.kind === "planning"}
          {#if entry.status === "running"}
            <p class="m-0 flex items-center gap-2 text-body-sm text-foreground">
              <Spinner size={12} class="shrink-0" />
              {$t("batch_view.activity.planning")}
            </p>
          {:else}
            <div class="space-y-1">
              <p class="m-0 flex items-center gap-2 text-body-sm text-foreground">
                <Sparkles size={14} strokeWidth={1.5} class="shrink-0 text-muted-foreground" />
                {$t("batch_view.activity.planned", {
                  values: { seconds: seconds(entry.durationMs) || "0 s", tokens: entry.tokens ?? 0 },
                })}
              </p>
              {#if entry.thinking !== ""}
                <div class="pl-5">
                  <ReasoningBlock
                    content={splitThinking(entry.thinking).thinking || entry.thinking}
                    durationMs={entry.durationMs}
                    expanded={open[index] === true}
                    ontoggle={() => toggle(index)}
                  />
                </div>
              {/if}
            </div>
          {/if}
        {:else if entry.kind === "plan"}
          <p class="m-0 flex items-center gap-2 text-body-sm text-foreground">
            <ListChecks size={14} strokeWidth={1.5} class="shrink-0 text-muted-foreground" />
            {$t("batch_view.activity.plan", { values: { count: entry.steps } })}
          </p>
        {:else if entry.kind === "step"}
          <div class="flex items-start gap-2 text-body-sm">
            <span class="mt-0.5 shrink-0">
              {#if entry.status === "running"}
                <Spinner size={12} />
              {:else if entry.status === "done"}
                <Check size={14} strokeWidth={1.75} class="text-success" />
              {:else}
                <X size={14} strokeWidth={1.75} class="text-danger-a11y" />
              {/if}
            </span>
            <div class="min-w-0">
              <p class="m-0 break-words text-foreground">
                {#if entry.num !== null && entry.total !== null}
                  <span class="text-muted-foreground">
                    {$t("batch_view.activity.step_of", { values: { num: entry.num, total: entry.total } })} ·
                  </span>
                {/if}
                {stepText(entry.desc, entry.target)}
                {#if seconds(entry.durationMs) !== ""}
                  <span class="text-caption text-muted-foreground"> · {seconds(entry.durationMs)}</span>
                {/if}
              </p>
              {#if entry.error !== null}
                <p class="m-0 mt-0.5 break-words text-caption text-danger-a11y">{entry.error}</p>
              {/if}
            </div>
          </div>
        {:else if entry.kind === "tool"}
          <div class="pl-5">
            <ToolStep
              item={asToolItem(entry, index)}
              {skin}
              expanded={open[index] === true}
              ontoggle={() => toggle(index)}
              sessionId={sessionId ?? undefined}
            />
          </div>
        {:else if entry.kind === "text"}
          {@const written = splitThinking(entry.text)}
          <div class="space-y-1 pl-5">
            {#if written.thinking !== ""}
              <ReasoningBlock content={written.thinking} expanded={open[index] === true} ontoggle={() => toggle(index)} />
            {/if}
            {#if written.answer !== ""}
              <p class="m-0 whitespace-pre-wrap break-words text-body-sm text-foreground">{written.answer}</p>
            {/if}
          </div>
        {:else if entry.kind === "model"}
          <p class="m-0 flex items-center gap-2 pl-5 text-caption text-muted-foreground">
            <Sparkles size={12} strokeWidth={1.5} class="shrink-0" />
            {$t("batch_view.activity.model", {
              values: { tokens: entry.tokens ?? 0, seconds: seconds(entry.latencyMs) || "0 s" },
            })}
          </p>
        {:else if entry.kind === "replan"}
          <p class="m-0 flex items-start gap-2 break-words text-body-sm text-warning-a11y">
            <RotateCcw size={14} strokeWidth={1.5} class="mt-0.5 shrink-0" />
            {$t("batch_view.activity.replan", { values: { reason: entry.reason } })}
          </p>
        {:else if entry.kind === "approval"}
          <p class="m-0 flex items-start gap-2 break-words text-body-sm text-foreground">
            <ShieldQuestion size={14} strokeWidth={1.5} class="mt-0.5 shrink-0 text-muted-foreground" />
            <span>
              {$t("batch_view.activity.approval", { values: { prompt: entry.prompt } })}
              <span class="text-caption text-muted-foreground">
                · {$t(
                  entry.approved === null
                    ? "batch_view.activity.approval_waiting"
                    : entry.approved
                      ? "batch_view.activity.approval_granted"
                      : "batch_view.activity.approval_refused",
                )}
              </span>
            </span>
          </p>
        {/if}
      </li>
    {/each}

    {#if activity.writing !== ""}
      <li class="space-y-1 pl-5" data-kind="writing" data-testid="batch-item-activity-writing">
        {#if now.thinking !== ""}
          <ReasoningBlock content={now.thinking} live={running && now.answer === ""} expanded={true} ontoggle={() => {}} />
        {/if}
        {#if now.answer !== ""}
          <p class="m-0 whitespace-pre-wrap break-words text-body-sm text-foreground">{now.answer}</p>
        {/if}
      </li>
    {:else if running}
      <li class="flex items-center gap-2 pl-5 text-caption text-muted-foreground" data-kind="waiting">
        <CircleDashed size={12} strokeWidth={1.5} class="shrink-0 animate-spin" />
        {$t("batch_view.activity.waiting_model")}
      </li>
    {/if}
  </ol>
{/if}
