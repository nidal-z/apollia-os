<script lang="ts">
  /**
   * Palette footer: the resting keyboard legend (navigate / open / close).
   * Purely informational, hidden from the a11y tree via `aria-hidden`.
   */
  import { t } from "svelte-i18n";
  import Keycap from "./Keycap.svelte";
</script>

<div class="footer" aria-hidden="true">
  <span class="hint">
    <Keycap label="↑" />
    <Keycap label="↓" />
    {$t("command.footer.navigate")}
  </span>
  <span class="hint">
    <Keycap label="↵" />
    {$t("command.footer.open")}
  </span>
  <span class="hint spacer">
    <Keycap label="esc" />
    {$t("command.footer.close")}
  </span>
</div>

<style>
  .footer {
    display: flex;
    align-items: center;
    gap: 0.875rem;
    padding: 0.5rem 0.75rem;
    border-top: 1px solid hsl(var(--border-soft));
    font-size: var(--text-caption);
    color: hsl(var(--muted-foreground));
  }

  .hint {
    display: inline-flex;
    align-items: center;
    gap: 0.3125rem;
  }

  .spacer {
    margin-left: auto;
  }
</style>
