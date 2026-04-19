<script lang="ts">
  import { EmptyState as CanonicalEmptyState } from "$lib/components/layout";
  import type { Icon } from "lucide-svelte";

  let {
    icon,
    title,
    subtitle,
    ctaLabel,
    ctaAction,
    page,
    class: klass = "",
  }: {
    icon: typeof Icon;
    title: string;
    subtitle?: string;
    ctaLabel?: string;
    ctaAction?: () => void;
    page?: string;
    class?: string;
  } = $props();
</script>

<CanonicalEmptyState
  {icon}
  {title}
  description={subtitle}
  primaryLabel={ctaLabel}
  primaryAction={ctaAction}
  {page}
  class={klass}
/>
