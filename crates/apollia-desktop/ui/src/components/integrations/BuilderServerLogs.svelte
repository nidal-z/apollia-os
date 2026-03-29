<script lang="ts">
  import { t } from "svelte-i18n";

  interface Props {
    serverName: string;
  }

  let { serverName }: Props = $props();
</script>

<div
  class="flex flex-col items-center justify-center py-10 gap-2"
  data-testid="logs-placeholder"
  data-server={serverName}
>
  <p class="max-w-xs text-center text-sm text-muted-foreground">
    {$t("integrations.builder.detail.logs_placeholder")}
  </p>
</div>
