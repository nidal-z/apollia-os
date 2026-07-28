<script lang="ts">
  /**
   * ProfileSourceBadge - provenance pill for a profile field or memory fact.
   *
   * Maps `written_by` to a token-only tint: onboarding -> primary, user ->
   * success, agent -> warning. Replaces the former emerald/amber Tailwind
   * literals, which duplicated the light/dark logic instead of letting the HSL
   * token carry it.
   */
  import { t } from "svelte-i18n";
  import { sourceLabelKey } from "./profileForm";

  interface Props {
    /** Raw `written_by`: `onboarding` | `user` | `agent:<name>`. */
    source: string | undefined;
  }

  let { source }: Props = $props();

  const labelKey = $derived(sourceLabelKey(source));

  const tintClass = $derived.by(() => {
    if (source === "onboarding") return "bg-primary/10 text-primary";
    if (source === "user") return "bg-success/10 text-success-a11y";
    if (source?.startsWith("agent:")) return "bg-warning/10 text-warning-a11y";
    return "bg-muted text-muted-foreground";
  });
</script>

{#if labelKey}
  <span
    class="inline-flex items-center rounded-full px-1.5 py-px text-overline font-semibold uppercase tracking-wide {tintClass}"
    title={$t("settings.profile.source.tooltip", {
      values: { source: $t(labelKey) },
    })}
  >
    {$t(labelKey)}
  </span>
{/if}
