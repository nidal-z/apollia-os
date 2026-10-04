<script lang="ts">
  /**
   * Body of a command: the command and what it printed, then how it ended
   * (finished or failed, its exit code, how long it took).
   */

  import type { ReasoningItem } from "$lib/chat/reasoning";
  import { parseBashOutput } from "$lib/chat/toolBodies";
  import { formatStepDuration } from "$lib/chat/toolBodyFormat";
  import BodyCode from "./BodyCode.svelte";
  import { t, locale } from "svelte-i18n";

  type ToolCallItem = Extract<ReasoningItem, { kind: "tool_call" }>;

  let { item }: { item: ToolCallItem } = $props();

  /** Lines of output shown at most. */
  const SHOWN = 20;

  const command = $derived(typeof item.args.command === "string" ? item.args.command : "");
  const parsed = $derived(parseBashOutput(item.output, item.exit_code));
  const failed = $derived(
    (parsed.exitCode !== null && parsed.exitCode !== 0) ||
      item.status === "error" ||
      item.status === "rejected",
  );

  function clip(text: string): string {
    return text.replace(/\n+$/, "").split("\n").slice(0, SHOWN).join("\n");
  }

  const ending = $derived.by<string>(() => {
    const parts = [failed ? $t("tools.body.command_failed") : $t("tools.body.command_finished")];
    if (parsed.exitCode != null) {
      parts.push($t("tools.body.exit_code_label", { values: { code: parsed.exitCode } }));
    }
    const duration = formatStepDuration(item.duration_ms, $locale ?? "en");
    if (duration) parts.push($t("tools.body.in_duration", { values: { duration } }));
    return parts.join(", ");
  });
</script>

<div class="flex flex-col gap-2" data-testid="tool-body-bash_executor">
  <BodyCode
    >$ {command}{#if parsed.body}{"\n"}<span class="text-muted-foreground">{clip(parsed.body)}</span
      >{/if}{#if parsed.stderr}{"\n"}<span class="text-destructive">{clip(parsed.stderr)}</span
      >{/if}</BodyCode
  >
  {#if item.output !== null}
    <p class="text-caption text-muted-foreground">{ending}</p>
  {/if}
</div>
