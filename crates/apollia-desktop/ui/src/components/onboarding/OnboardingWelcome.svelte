<script lang="ts">
  /**
   * Onboarding step 1 — Welcome screen.
   *
   * Static intro card that introduces Apollia and triggers the step machine.
   * Designed to fit inside the shared {@link OnboardingModal} overlay (no
   * own backdrop or fixed positioning).
   *
   * Includes a top-right language toggle so the user can switch FR/EN
   * before any screen of the wizard is filled — once a profile is set the
   * locale is harder to change without re-entering the welcome page.
   */
  import { ShieldCheck, Cpu, Sparkles, Languages } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { t, locale } from "svelte-i18n";
  import { setLocale, SUPPORTED_LOCALES } from "$lib/i18n";

  interface Props {
    onnext: () => void;
  }

  const { onnext }: Props = $props();

  function changeLocale(next: string): void {
    setLocale(next);
    locale.set(next);
  }
</script>

<div
  class="relative flex flex-col items-center gap-6 px-2 py-4 text-center"
  data-testid="onboarding-welcome"
>
  <!-- Language toggle (top-right) -->
  <div
    class="absolute right-2 top-2 inline-flex items-center gap-1 rounded-full border border-border/60 bg-card/80 px-1 py-0.5 text-[10px] backdrop-blur"
    data-testid="onboarding-lang-toggle"
  >
    <Languages size={12} aria-hidden="true" class="ml-1.5 text-muted-foreground" />
    {#each SUPPORTED_LOCALES as code}
      <button
        type="button"
        class="rounded-full px-2 py-0.5 font-semibold uppercase transition-colors {$locale === code
          ? 'bg-primary text-primary-foreground'
          : 'text-muted-foreground hover:bg-muted'}"
        onclick={() => changeLocale(code)}
        data-testid={`onboarding-lang-${code}`}
        aria-label={`Switch to ${code.toUpperCase()}`}
      >
        {code}
      </button>
    {/each}
  </div>

  <img
    src="/logo.svg"
    alt="Apollia OS"
    class="h-20 w-20 object-contain drop-shadow-[0_8px_24px_hsl(var(--primary)/0.35)]"
  />

  <div class="space-y-2">
    <h2 class="text-xl font-semibold tracking-tight text-foreground">
      {$t("onboarding_welcome.title")}
    </h2>
    <p class="mx-auto max-w-sm text-sm leading-relaxed text-muted-foreground">
      {$t("onboarding_welcome.tagline")}
    </p>
  </div>

  <ul class="grid w-full max-w-md grid-cols-1 gap-2 text-left sm:grid-cols-3">
    <li class="flex items-start gap-2 rounded-lg border border-border/60 bg-muted/40 p-3">
      <ShieldCheck size={16} strokeWidth={1.75} class="mt-0.5 flex-shrink-0 text-primary" />
      <div>
        <p class="text-xs font-medium text-foreground">
          {$t("onboarding_welcome.feature_local_title")}
        </p>
        <p class="text-[11px] leading-snug text-muted-foreground">
          {$t("onboarding_welcome.feature_local_desc")}
        </p>
      </div>
    </li>
    <li class="flex items-start gap-2 rounded-lg border border-border/60 bg-muted/40 p-3">
      <Cpu size={16} strokeWidth={1.75} class="mt-0.5 flex-shrink-0 text-primary" />
      <div>
        <p class="text-xs font-medium text-foreground">
          {$t("onboarding_welcome.feature_llm_title")}
        </p>
        <p class="text-[11px] leading-snug text-muted-foreground">
          {$t("onboarding_welcome.feature_llm_desc")}
        </p>
      </div>
    </li>
    <li class="flex items-start gap-2 rounded-lg border border-border/60 bg-muted/40 p-3">
      <Sparkles size={16} strokeWidth={1.75} class="mt-0.5 flex-shrink-0 text-primary" />
      <div>
        <p class="text-xs font-medium text-foreground">
          {$t("onboarding_welcome.feature_agents_title")}
        </p>
        <p class="text-[11px] leading-snug text-muted-foreground">
          {$t("onboarding_welcome.feature_agents_desc")}
        </p>
      </div>
    </li>
  </ul>

  <Button
    variant="primary-gradient"
    size="lg"
    onclick={onnext}
    data-testid="onboarding-welcome-cta"
  >
    {$t("onboarding_welcome.cta_start")}
  </Button>
</div>
