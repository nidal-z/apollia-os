<script lang="ts">
  /**
   * Per-tool expanded body for the `plan_*` tool family, branched by tool.
   *
   * Operator layer: a friendly, jargon-free action line describing what THIS
   * call did ("Plan proposed, N steps", "Step added: …", "Step X -> status"),
   * plus the affected step(s) as a title + status pill where derivable. The full
   * plan renders in the dedicated plan host; this body only reflects the delta.
   * Builder layer: the raw args and output JSON. Everything degrades to the raw
   * output on a shape mismatch, and an `ok: false` result shows a failure line.
   */

  import type { ReasoningItem } from "$lib/chat/reasoning";
  import {
    parsePlanCall,
    stringifyValue,
    prettyJson,
    type PlanStepLite,
  } from "$lib/chat/toolBodies";
  import { t } from "svelte-i18n";
  import { fly, fade } from "svelte/transition";
  import { Circle, CircleDot, CheckCircle2, MinusCircle, XCircle } from "lucide-svelte";

  type ToolCallItem = Extract<ReasoningItem, { kind: "tool_call" }>;

  interface Props {
    item: ToolCallItem;
    skin: "builder" | "operator";
  }

  let { item, skin }: Props = $props();

  const info = $derived(parsePlanCall(item.args, item.output));

  function statusKey(status: string | null): string {
    switch (status) {
      case "in_progress":
        return "status_in_progress";
      case "completed":
        return "status_completed";
      case "skipped":
        return "status_skipped";
      case "failed":
        return "status_failed";
      default:
        return "status_pending";
    }
  }

  function pillClass(status: string | null): string {
    switch (status) {
      case "in_progress":
        return "tb-active";
      case "completed":
        return "tb-ok";
      case "failed":
        return "tb-error";
      default:
        return "";
    }
  }

  // Steps to show under the action line: the resulting summaries for whole-plan
  // calls, else the single affected step for add / modify.
  const shownSteps = $derived.by<PlanStepLite[]>(() => {
    if (item.tool === "plan_add_step" || item.tool === "plan_modify_step") {
      return info.argStep ? [info.argStep] : [];
    }
    if (item.tool === "plan_propose" || item.tool === "plan_submit") {
      return info.outputSteps ?? [];
    }
    return [];
  });

  const actionLine = $derived.by<string>(() => {
    if (!info.ok) return $t("tools.body.plan_failed");
    switch (item.tool) {
      case "plan_propose":
        return $t("tools.body.plan_proposed", {
          values: {
            n: info.proposeCount ?? info.outputSteps?.length ?? 0,
          },
        });
      case "plan_add_step":
        return $t("tools.body.plan_step_added", {
          values: { title: info.argStep?.title ?? info.argStep?.stepId ?? "" },
        });
      case "plan_modify_step":
        return $t("tools.body.plan_step_modified", {
          values: { title: info.argStep?.title ?? info.stepId ?? "" },
        });
      case "plan_remove_step":
        return $t("tools.body.plan_step_removed");
      case "plan_reorder":
        return $t("tools.body.plan_reordered", {
          values: { n: info.orderCount ?? info.outputSteps?.length ?? 0 },
        });
      case "plan_submit":
        return $t("tools.body.plan_submitted");
      case "plan_set_step_status":
        return $t("tools.body.plan_status_changed", {
          values: {
            id: info.stepId ?? "",
            status: $t(`tools.body.${statusKey(info.status)}`),
          },
        });
      default:
        return $t("tools.body.plan_generic");
    }
  });

  // Progress "done / total" when this call returned the whole plan (propose /
  // submit). Delta-only calls (add / modify / status) carry no full list.
  const planProgress = $derived.by<{ done: number; total: number } | null>(() => {
    const steps = info.outputSteps;
    if (!steps || steps.length === 0) return null;
    const done = steps.filter((s) => s.status === "completed").length;
    return { done, total: steps.length };
  });

  const inputJson = $derived(stringifyValue(item.args));
  const outputJson = $derived(prettyJson(item.output));
</script>

<div class="text-[12px]" in:fly={{ y: 8, duration: 200 }}>
  {#key skin}
    <div in:fade={{ duration: 150 }}>
      {#if skin === "operator"}
        <div class="flex flex-col gap-2">
          <p class="text-foreground">{actionLine}</p>
          {#if planProgress}
            <p class="tb-metric">
              {$t("tools.body.progress_fraction", {
                values: { done: planProgress.done, total: planProgress.total },
              })}
            </p>
          {/if}
          {#if shownSteps.length > 0}
            <div class="tb-steps">
              {#each shownSteps as step (step.stepId)}
                <div class="tb-step">
                  <span class="tb-step-glyph {pillClass(step.status)}" aria-hidden="true">
                    {#if step.status === "completed"}
                      <CheckCircle2 size={14} />
                    {:else if step.status === "in_progress"}
                      <CircleDot size={14} />
                    {:else if step.status === "failed"}
                      <XCircle size={14} />
                    {:else if step.status === "skipped"}
                      <MinusCircle size={14} />
                    {:else}
                      <Circle size={14} />
                    {/if}
                  </span>
                  <span class="tb-step-title {step.status === 'completed' ? 'tb-done' : ''}"
                    >{step.title || step.stepId}</span
                  >
                  {#if step.status}
                    <span class="tb-pill tb-step-pill {pillClass(step.status)}"
                      >{$t(`tools.body.${statusKey(step.status)}`)}</span
                    >
                  {/if}
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {:else}
        <div class="flex flex-col gap-2">
          <div class="flex flex-col gap-1">
            <div class="tb-iolabel">
              {$t("chat.reasoning.input_label", { default: "Input" })}
            </div>
            <pre class="tb-code font-mono"><code>{inputJson}</code></pre>
          </div>
          {#if outputJson}
            <div class="flex flex-col gap-1">
              <div class="tb-iolabel">
                {$t("chat.reasoning.output_label", { default: "Output" })}
              </div>
              <pre class="tb-code font-mono"><code>{outputJson}</code></pre>
            </div>
          {/if}
        </div>
      {/if}
    </div>
  {/key}
</div>
