<script lang="ts">
  /**
   * Compact badge stating whether a group of shortcuts is listened to globally
   * or only inside a mounted surface. Rendered in a section header when every
   * entry in the group shares one scope.
   */
  import { t } from "svelte-i18n";
  import { Badge } from "$lib/components/ui/badge";
  import type { ShortcutScope } from "$lib/navigation/shortcuts";

  interface Props {
    scope: ShortcutScope;
  }

  let { scope }: Props = $props();
</script>

{#if scope === "global"}
  <Badge variant="primary" size="sm" outline>
    {$t("settings.shortcuts.scope.global_label")}
  </Badge>
{:else}
  <Badge variant="neutral" size="sm">
    {$t("settings.shortcuts.scope.contextual_label")}
  </Badge>
{/if}
