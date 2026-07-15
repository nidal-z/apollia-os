<script lang="ts">
  import Toast from "./Toast.svelte";
  import { visibleToasts, queuedCount, removeToast } from "./store";
  import { t } from "svelte-i18n";
</script>

<div
  class="fixed top-4 right-4 flex flex-col gap-2 pointer-events-none"
  style="z-index: var(--z-toast, 40);"
  data-testid="toast-container"
>
  {#each $visibleToasts as toast (toast.id)}
    <Toast
      message={toast.message}
      variant={toast.variant}
      autoDismiss={toast.autoDismiss}
      showProgress={toast.showProgress}
      ondismiss={() => removeToast(toast.id)}
      actionLabel={toast.actionLabel}
      onaction={toast.onaction}
      data-testid={toast["data-testid"] ?? "toast"}
    />
  {/each}
  {#if $queuedCount > 0}
    <div
      class="pointer-events-auto self-end rounded-md border border-border bg-card/90 px-2.5 py-1 text-xs text-muted-foreground shadow-sm backdrop-blur"
      role="status"
      aria-live="polite"
      data-testid="toast-more"
    >
      {$t("toast.more", { values: { count: $queuedCount } })}
    </div>
  {/if}
</div>
