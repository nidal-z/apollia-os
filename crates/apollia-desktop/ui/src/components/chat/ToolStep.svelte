<script lang="ts">
  /**
   * One tool step of an assistant turn: a single foldable line (the action in
   * plain words, a short target, the state and the duration) that unfolds on
   * the tool's own body.
   *
   * The bodies are readable by anyone and show in both modes. Developer mode
   * adds the raw data of the step under its body, never in place of it.
   */

  import type { ReasoningItem } from "$lib/chat/reasoning";
  import { stepActionKey, stepIcon, stepState, stepTarget, toolCallView } from "$lib/chat/toolStep";
  import { formatSecondsLocalized } from "$lib/chat/duration";
  import { resolveToolDisplay, humanizeToolError, humanizeToolName, formatRationale } from "$lib/tools/tool-display";
  import { t, locale } from "svelte-i18n";
  import { Check, ChevronRight, TriangleAlert, X } from "lucide-svelte";
  import { Spinner } from "$lib/components/ui/progress";
  import PerformanceHint from "./PerformanceHint.svelte";
  import RetryTimeline from "./RetryTimeline.svelte";
  import WebSearchBody from "./tool-bodies/WebSearchBody.svelte";
  import WebReadBody from "./tool-bodies/WebReadBody.svelte";
  import FileListBody from "./tool-bodies/FileListBody.svelte";
  import FileReadBody from "./tool-bodies/FileReadBody.svelte";
  import FileWriteBody from "./tool-bodies/FileWriteBody.svelte";
  import FileGlobBody from "./tool-bodies/FileGlobBody.svelte";
  import FileGrepBody from "./tool-bodies/FileGrepBody.svelte";
  import BashBody from "./tool-bodies/BashBody.svelte";
  import PythonBody from "./tool-bodies/PythonBody.svelte";
  import HttpFetchBody from "./tool-bodies/HttpFetchBody.svelte";
  import MemorySearchBody from "./tool-bodies/MemorySearchBody.svelte";
  import TodoBody from "./tool-bodies/TodoBody.svelte";
  import AskUserBody from "./tool-bodies/AskUserBody.svelte";
  import GenericBody from "./tool-bodies/GenericBody.svelte";
  import ToolRawData from "./tool-bodies/ToolRawData.svelte";
  import ConnectionOfferCards from "./ConnectionOfferCards.svelte";
  import { CONNECTION_LOOKUP_TOOL } from "$lib/chat/connectionOffer";

  type ToolCallItem = Extract<ReasoningItem, { kind: "tool_call" }>;

  interface Props {
    item: ToolCallItem;
    /** `builder` is developer mode: the raw data shows under the body. */
    skin: "builder" | "operator";
    expanded: boolean;
    ontoggle: () => void;
    /** Forwarded to the file bodies, so a relative path can be revealed. */
    sessionId?: string;
  }

  let { item, skin, expanded, ontoggle, sessionId }: Props = $props();

  const phase = $derived(stepState(item.status));
  const Icon = $derived(stepIcon(item));

  const action = $derived.by<string>(() => {
    const key = stepActionKey(item.tool);
    if (key) return $t(key);
    const display = resolveToolDisplay(toolCallView(item));
    return $t(display.labelKey, {
      values: display.templateParams,
      default: humanizeToolName(item.tool),
    });
  });

  const target = $derived.by<string>(() => {
    const found = stepTarget(item);
    if (!found) return "";
    return "text" in found ? found.text : $t(found.key, { values: found.values });
  });

  const duration = $derived(
    item.duration_ms != null && item.duration_ms > 0
      ? `${formatSecondsLocalized(item.duration_ms, $locale ?? "en")} s`
      : "",
  );

  const failure = $derived.by<string>(() => {
    if (phase === "refused") return $t("chat.step.refused");
    if (phase !== "error") return "";
    const reason = item.output ? (humanizeToolError(item.output).split("\n")[0] ?? "").slice(0, 240) : "";
    return reason ? $t("chat.step.failed", { values: { reason } }) : $t("chat.step.failed_plain");
  });

  const rationale = $derived(formatRationale(item.rationale));
  const retryAttempts = $derived(item.retry_attempts ?? []);
  const retryCount = $derived(Math.max(0, retryAttempts.length - 1));
</script>

<div class="flex flex-col" data-testid="reasoning-card-tool_call" data-state={phase}>
  <button
    type="button"
    class="flex w-full min-w-0 items-center gap-2 rounded-md py-1 text-left text-body-sm transition-colors hover:bg-surface-2/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    aria-expanded={expanded}
    aria-label={$t("chat.step.toggle")}
    data-testid="tool-step-toggle"
    onclick={ontoggle}
  >
    <ChevronRight
      class="h-3.5 w-3.5 flex-none text-muted-foreground transition-transform duration-fast {expanded ? 'rotate-90' : ''}"
      aria-hidden="true"
    />
    <span class="flex-none {phase === 'error' ? 'text-destructive' : 'text-muted-foreground'}" aria-hidden="true">
      <Icon class="h-3.5 w-3.5" />
    </span>
    <span class="flex-none font-medium text-foreground" data-testid="tool-step-action">{action}</span>
    {#if target}
      <span class="min-w-0 truncate text-body-xs text-muted-foreground" data-testid="tool-step-target">{target}</span>
    {/if}
    <span class="ml-auto flex flex-none items-center gap-1.5 pl-2 text-caption tabular-nums text-muted-foreground">
      {#if rationale?.performance_hint}
        <PerformanceHint hint={rationale.performance_hint} />
      {/if}
      {#if retryCount > 0}
        <RetryTimeline attempts={retryAttempts} skin="operator" />
      {/if}
      {#if phase === "running"}
        <Spinner size={13} class="text-primary" label={$t("chat.step.running")} />
      {:else if phase === "success"}
        <Check class="h-3.5 w-3.5 text-success" aria-hidden="true" />
      {:else if phase === "error"}
        <TriangleAlert class="h-3.5 w-3.5 text-destructive" aria-hidden="true" />
      {:else}
        <X class="h-3.5 w-3.5 text-destructive" aria-hidden="true" />
      {/if}
      {#if duration}
        <span data-testid="tool-step-duration">{duration}</span>
      {/if}
    </span>
  </button>

  {#if failure}
    <p class="pl-6 text-body-xs text-destructive" data-testid="tool-step-failure">{failure}</p>
  {/if}

  <!-- What Apollia found missing is offered folded or not: it is what to do next. -->
  {#if item.tool === CONNECTION_LOOKUP_TOOL && phase === "success" && sessionId}
    <ConnectionOfferCards output={item.output} {sessionId} />
  {/if}

  {#if expanded}
    <div
      class="mt-1 flex flex-col gap-2 rounded-lg border border-border/60 bg-card px-3 py-2.5"
      data-testid="tool-step-body"
    >
      {#if rationale?.summary}
        <p class="text-body-xs italic text-muted-foreground">{rationale.summary}</p>
      {/if}
      {#if item.tool === "web_search"}
        <WebSearchBody {item} />
      {:else if item.tool === "web_read"}
        <WebReadBody {item} />
      {:else if item.tool === "file_list"}
        <FileListBody {item} {sessionId} />
      {:else if item.tool === "file_read"}
        <FileReadBody {item} {sessionId} />
      {:else if item.tool === "file_write" || item.tool === "file_edit"}
        <FileWriteBody {item} {sessionId} />
      {:else if item.tool === "file_glob"}
        <FileGlobBody {item} {sessionId} />
      {:else if item.tool === "file_grep"}
        <FileGrepBody {item} {sessionId} />
      {:else if item.tool === "bash_executor"}
        <BashBody {item} />
      {:else if item.tool === "python_executor"}
        <PythonBody {item} />
      {:else if item.tool === "http_fetch"}
        <HttpFetchBody {item} />
      {:else if item.tool === "memory_search"}
        <MemorySearchBody {item} />
      {:else if item.tool === "todo_write"}
        <TodoBody {item} />
      {:else if item.tool === "ask_user"}
        <AskUserBody {item} />
      {:else}
        <GenericBody {item} />
      {/if}
      {#if retryAttempts.length > 0 && (skin === "builder" || retryCount > 0)}
        <RetryTimeline attempts={retryAttempts} {skin} />
      {/if}
      {#if skin === "builder"}
        <ToolRawData {item} />
      {/if}
    </div>
  {/if}
</div>
