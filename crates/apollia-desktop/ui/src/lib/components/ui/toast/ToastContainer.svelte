<script lang="ts">
  import Toast from "./Toast.svelte";
  import { toasts, removeToast } from "./store";
</script>

<div class="fixed top-4 right-4 z-[100] flex flex-col gap-2 pointer-events-none">
  {#each $toasts as toast (toast.id)}
    <Toast
      message={toast.message}
      variant={toast.variant}
      ondismiss={() => removeToast(toast.id)}
      actionLabel={toast.actionLabel}
      onaction={toast.onaction}
      data-testid={toast["data-testid"] ?? "toast"}
    />
  {/each}
</div>
