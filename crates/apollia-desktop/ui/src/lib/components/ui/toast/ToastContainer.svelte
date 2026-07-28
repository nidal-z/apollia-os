<script lang="ts">
  import Toast from "./Toast.svelte";
  import { visibleToasts, queuedCount, removeToast } from "./store";
  import { t } from "svelte-i18n";
</script>

<div
  class="pointer-events-none fixed top-4 right-4 flex w-full max-w-[26.25rem] flex-col items-end gap-2"
  style="z-index: var(--z-toast, 40);"
  data-testid="toast-container"
>
  {#each $visibleToasts as toast (toast.id)}
    <Toast
      message={toast.message}
      description={toast.description}
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
      class="pointer-events-auto self-end rounded-md border border-border bg-card/90 px-2.5 py-1 text-xs text-muted-foreground shadow-elev-1 backdrop-blur"
      role="status"
      aria-live="polite"
      data-testid="toast-more"
    >
      {$t("toast.more", { values: { count: $queuedCount } })}
    </div>
  {/if}
</div>
