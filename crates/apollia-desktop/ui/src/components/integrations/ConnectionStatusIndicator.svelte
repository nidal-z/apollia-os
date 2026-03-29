<script lang="ts">
  import { t } from "svelte-i18n";

  interface Props {
    connected: boolean;
    error?: string | null;
  }

  let { connected, error = null }: Props = $props();

  type StatusConfig = { dot: string; labelKey: string };

  const status = $derived<StatusConfig>(
    connected
      ? { dot: "bg-emerald-500",  labelKey: "integrations.status.connected" }
      : error
        ? { dot: "bg-red-500",    labelKey: "integrations.status.error" }
        : { dot: "bg-gray-400",   labelKey: "integrations.status.disconnected" },
  );
</script>

<span class="inline-flex items-center gap-1.5">
  <span class="inline-block h-2 w-2 rounded-full {status.dot}" aria-hidden="true"></span>
  <span class="text-xs text-muted-foreground">{$t(status.labelKey)}</span>
</span>
