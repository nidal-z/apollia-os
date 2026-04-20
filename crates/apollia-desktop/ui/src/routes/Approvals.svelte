<script lang="ts">
  /**
   * Legacy `/approvals` surface — replaced by the unified `/inbox`
   * (US-SP42-053). Retained as a redirect so that deep links, sidebar
   * entries registered before the rewrite, and external references
   * continue to work without a hard error.
   */
  import { onMount } from "svelte";
  import { navigateTo } from "$lib/stores/navigation";

  onMount(() => {
    navigateTo("inbox");
  });
</script>

<p class="p-6 text-sm text-muted-foreground" data-testid="approvals-redirect">
  Redirecting to inbox…
</p>
