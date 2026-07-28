<script lang="ts">
  /**
   * SensitiveBadge - calm warn pill flagging a field that persists potentially
   * sensitive content (raw LLM prompts, debug prompt text).
   *
   * Deliberately warn-toned, never destructive red: capturing this data is a
   * legitimate local-first choice, the badge only asks for informed attention.
   */
  import { AlertTriangle } from "lucide-svelte";

  interface Props {
    label: string;
  }

  let { label }: Props = $props();
</script>

<span
  class="inline-flex items-center gap-1 rounded-full border border-warning/30 bg-warning/10 px-2 py-0.5 text-overline uppercase text-warning"
>
  <AlertTriangle size={10} strokeWidth={2} aria-hidden="true" />
  {label}
</span>
