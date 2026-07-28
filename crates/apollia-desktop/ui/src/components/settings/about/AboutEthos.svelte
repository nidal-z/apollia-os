<script lang="ts">
  /**
   * AboutEthos - the sovereignty line: the product's thesis, said once.
   *
   * A crafted card with a soft signature-gradient wash and a gradient badge,
   * all composed from existing HSL tokens.
   */
  import { t } from "svelte-i18n";
  import { Lock } from "lucide-svelte";
</script>

<div class="ethos-tint flex items-center gap-3.5 rounded-xl border px-4 py-3.5" data-testid="about-ethos">
  <span
    class="ethos-badge grid h-8 w-8 shrink-0 place-items-center rounded-lg text-primary-foreground shadow-primary-md"
    aria-hidden="true"
  >
    <Lock size={16} strokeWidth={1.9} />
  </span>
  <p class="m-0 text-body-sm text-foreground">{$t("settings.about.ethos")}</p>
</div>

<style>
  .ethos-tint {
    border-color: hsl(var(--grad-a) / 0.3);
    background: linear-gradient(
      100deg,
      hsl(var(--grad-a) / 0.1),
      hsl(var(--grad-b) / 0.05) 60%,
      transparent
    );
  }

  .ethos-badge {
    background: linear-gradient(140deg, hsl(var(--grad-a)), hsl(var(--grad-b)));
  }
</style>
