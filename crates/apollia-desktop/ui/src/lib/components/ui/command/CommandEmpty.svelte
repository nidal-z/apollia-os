<script lang="ts">
  /** No-results state for the palette. Announced via `role="status"`. */
  import { t } from "svelte-i18n";
  import { SearchX } from "lucide-svelte";
</script>

<div class="empty" role="status">
  <span class="empty-badge" aria-hidden="true">
    <SearchX size={20} strokeWidth={1.75} />
  </span>
  {$t("command.empty")}
  <span class="empty-hint">{$t("command.empty_hint")}</span>
</div>

<style>
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 2.5rem 0.75rem;
    text-align: center;
    font-size: 0.84375rem;
    color: hsl(var(--muted-foreground));
  }

  .empty-badge {
    display: grid;
    place-items: center;
    width: 2.75rem;
    height: 2.75rem;
    margin-bottom: 0.75rem;
    border-radius: 50%;
    background: hsl(var(--muted) / 0.7);
    color: hsl(var(--muted-foreground));
  }

  .empty-hint {
    margin-top: 0.375rem;
    font-size: 0.71875rem;
    color: hsl(var(--muted-foreground) / 0.85);
  }
</style>
