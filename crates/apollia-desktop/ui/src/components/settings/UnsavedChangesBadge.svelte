<script lang="ts">
  import { t } from "svelte-i18n";
  import { Badge } from "$lib/components/ui/badge";
  import { hasExplicitDirty } from "$lib/stores/settingsDirty";
</script>

{#if $hasExplicitDirty}
  <Badge variant="warning" size="sm" data-testid="settings-unsaved-badge">
    <span class="relative inline-flex h-1.5 w-1.5">
      <span class="absolute inline-flex h-full w-full animate-ping rounded-full bg-warning opacity-75"></span>
      <span class="relative inline-flex h-1.5 w-1.5 rounded-full bg-warning"></span>
    </span>
    <span class="ml-1.5">{$t('settings.unsaved_changes')}</span>
  </Badge>
{/if}
