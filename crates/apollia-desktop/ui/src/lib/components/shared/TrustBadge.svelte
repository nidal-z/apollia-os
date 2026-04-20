<script lang="ts" module>
  import type { TrustLevel } from "$lib/types";

  export type TrustVisual = "verified" | "community" | "unverified" | "unknown";

  export function toTrustVisual(level: TrustLevel | null | undefined): TrustVisual {
    switch (level) {
      case "verified_official":
      case "community_verified":
        return "verified";
      case "community":
        return "community";
      case "custom":
        return "unverified";
      default:
        return "unknown";
    }
  }
</script>

<script lang="ts">
  import { t } from "svelte-i18n";
  import { ShieldCheck, Users, Info, HelpCircle } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";

  interface Props {
    /** Canonical trust level (from backend). */
    level?: TrustLevel | null;
    /** Direct 4-level visual override (prefer `level` for data-driven usage). */
    visual?: TrustVisual;
    size?: "sm" | "md";
  }

  let { level = null, visual, size = "sm" }: Props = $props();

  const resolved = $derived<TrustVisual>(visual ?? toTrustVisual(level));

  type Config = {
    variant: "success" | "info" | "warning" | "neutral";
    icon: typeof ShieldCheck;
    labelKey: string;
    tooltipKey: string;
  };

  const CONFIG: Record<TrustVisual, Config> = {
    verified: {
      variant: "success",
      icon: ShieldCheck,
      labelKey: "trust.verified.label",
      tooltipKey: "trust.verified.tooltip",
    },
    community: {
      variant: "info",
      icon: Users,
      labelKey: "trust.community.label",
      tooltipKey: "trust.community.tooltip",
    },
    unverified: {
      variant: "warning",
      icon: Info,
      labelKey: "trust.unverified.label",
      tooltipKey: "trust.unverified.tooltip",
    },
    unknown: {
      variant: "neutral",
      icon: HelpCircle,
      labelKey: "trust.unknown.label",
      tooltipKey: "trust.unknown.tooltip",
    },
  };

  const config = $derived(CONFIG[resolved]);
  const iconSize = $derived(size === "md" ? 12 : 10);
</script>

<Badge
  variant={config.variant}
  {size}
  title={$t(config.tooltipKey)}
  data-testid="trust-badge"
  data-trust={resolved}
>
  {#snippet icon()}
    <config.icon size={iconSize} strokeWidth={2} />
  {/snippet}
  {$t(config.labelKey)}
</Badge>
