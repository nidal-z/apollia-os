<script lang="ts">
  import { t } from "svelte-i18n";
  import { Badge } from "$lib/components/ui/badge";
  import type { TrustLevel } from "$lib/types";

  interface Props {
    level: TrustLevel;
  }

  const BADGE_CONFIG: Record<TrustLevel, { variant: "success" | "info" | "warning" | "secondary"; labelKey: string }> = {
    verified_official: { variant: "success", labelKey: "integrations.trust.verified_official" },
    community_verified: { variant: "info",    labelKey: "integrations.trust.community_verified" },
    community:         { variant: "warning",  labelKey: "integrations.trust.community" },
    custom:            { variant: "secondary", labelKey: "integrations.trust.custom" },
  };

  let { level }: Props = $props();

  const config = $derived(BADGE_CONFIG[level] ?? BADGE_CONFIG.community);
</script>

<Badge variant={config.variant}>
  {$t(config.labelKey)}
</Badge>
