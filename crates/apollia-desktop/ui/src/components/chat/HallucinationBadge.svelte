<script lang="ts">
  import { t } from "svelte-i18n";
  import { AlertOctagon } from "lucide-svelte";

  interface Props {
    /** Optional details surfaced in the native tooltip. */
    detail?: string;
  }

  let { detail }: Props = $props();

  const tooltip = $derived(
    detail ??
      $t("chat.hallucination.tooltip", {
        default:
          "The output looks fabricated (null, empty, or schema mismatch). Inspect the raw payload before trusting it.",
      })
  );
</script>

<span
  class="hallucination-badge"
  title={tooltip}
  data-testid="hallucination-badge"
  role="status"
>
  <AlertOctagon size={12} />
  <span class="label">
    {$t("chat.hallucination.label", { default: "Hallucination suspected" })}
  </span>
</span>

<style>
  .hallucination-badge {
    display: inline-flex;
    align-items: center;
    gap: 0.375rem;
    padding: 0.125rem 0.5rem;
    border-radius: 9999px;
    border: 1px solid hsl(var(--destructive) / 0.55);
    background-color: hsl(var(--destructive) / 0.12);
    color: hsl(var(--destructive));
    font-size: 11px;
    font-weight: 600;
    line-height: 1.4;
    cursor: help;
  }

  .label {
    white-space: nowrap;
  }
</style>
