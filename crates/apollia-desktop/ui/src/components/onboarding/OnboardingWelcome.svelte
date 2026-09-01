<script lang="ts">
  /**
   * Onboarding step 1 - Welcome screen (first-run hero).
   *
   * Static intro card that introduces Apollia and triggers the step machine.
   * Designed to fit inside the shared {@link OnboardingModal} overlay (no own
   * backdrop or fixed positioning).
   *
   * As the vitrine's first moment, this leans into the expressive-depth
   * tokens: a breathing signature-gradient halo behind the mark, a spring
   * pop-in, a staggered feature cascade, and a pulsed primary CTA. All motion
   * collapses under reduced-motion.
   *
   * Includes a top-right language toggle so the user can switch FR/EN before
   * any screen of the wizard is filled - once a profile is set the locale is
   * harder to change without re-entering the welcome page.
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
    class="absolute right-2 top-2 inline-flex items-center gap-1 rounded-full leading-none border border-border/60 bg-card/80 px-1 py-0.5 backdrop-blur"
    data-testid="onboarding-lang-toggle"
  >
    <Languages size={12} aria-hidden="true" class="ml-1.5 text-muted-foreground" />
    {#each SUPPORTED_LOCALES as code}
      <button
        type="button"
        class="rounded-full px-2 py-0.5 text-overline font-semibold uppercase transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary {$locale ===
        code
          ? 'bg-primary text-primary-foreground'
          : 'text-muted-foreground hover:bg-muted'}"
        onclick={() => changeLocale(code)}
        data-testid={`onboarding-lang-${code}`}
        aria-label={$t("onboarding_welcome.switch_locale", {
          values: { code: code.toUpperCase() },
        })}
      >
        {code}
      </button>
    {/each}
  </div>

  <div class="onboarding-mark" aria-hidden="true">
    <span class="onboarding-halo"></span>
    <img src="/logo.svg" alt="Apollia OS" class="onboarding-logo" />
  </div>

  <div class="space-y-2">
    <h2 class="text-display-sm text-foreground">
      {$t("onboarding_welcome.title")}
    </h2>
    <p class="mx-auto max-w-sm text-body-sm leading-relaxed text-muted-foreground">
      {$t("onboarding_welcome.tagline")}
    </p>
  </div>

  <ul class="grid w-full max-w-md grid-cols-1 gap-2 text-left sm:grid-cols-3">
    <li class="onboarding-feat" style="--feat-delay: 100ms">
      <ShieldCheck size={16} strokeWidth={1.75} class="mt-0.5 flex-shrink-0 text-primary" />
      <div>
        <p class="text-body-xs font-medium text-foreground">
          {$t("onboarding_welcome.feature_local_title")}
        </p>
        <p class="text-caption leading-snug text-muted-foreground">
          {$t("onboarding_welcome.feature_local_desc")}
        </p>
      </div>
    </li>
    <li class="onboarding-feat" style="--feat-delay: 180ms">
      <Cpu size={16} strokeWidth={1.75} class="mt-0.5 flex-shrink-0 text-primary" />
      <div>
        <p class="text-body-xs font-medium text-foreground">
          {$t("onboarding_welcome.feature_llm_title")}
        </p>
        <p class="text-caption leading-snug text-muted-foreground">
          {$t("onboarding_welcome.feature_llm_desc")}
        </p>
      </div>
    </li>
    <li class="onboarding-feat" style="--feat-delay: 260ms">
      <Sparkles size={16} strokeWidth={1.75} class="mt-0.5 flex-shrink-0 text-primary" />
      <div>
        <p class="text-body-xs font-medium text-foreground">
          {$t("onboarding_welcome.feature_agents_title")}
        </p>
        <p class="text-caption leading-snug text-muted-foreground">
          {$t("onboarding_welcome.feature_agents_desc")}
        </p>
      </div>
    </li>
  </ul>

  <Button
    variant="primary-gradient"
    size="lg"
    class="onboarding-cta-pulse"
    onclick={onnext}
    data-testid="onboarding-welcome-cta"
  >
    {$t("onboarding_welcome.cta_start")}
  </Button>
</div>

<style>
  .onboarding-mark {
    position: relative;
    width: 5rem;
    height: 5rem;
    display: grid;
    place-items: center;
  }

  .onboarding-halo {
    position: absolute;
    inset: -1.125rem;
    border-radius: 50%;
    background: radial-gradient(circle, hsl(var(--grad-b) / 0.45), transparent 68%);
    animation: onboarding-halo 2.4s var(--ease-standard) infinite alternate;
  }

  .onboarding-logo {
    position: relative;
    width: 5rem;
    height: 5rem;
    object-fit: contain;
    filter: drop-shadow(0 8px 24px hsl(var(--primary) / 0.35));
    animation: onboarding-pop var(--motion-slow) var(--ease-spring) both;
  }

  @keyframes onboarding-halo {
    from {
      opacity: 0.55;
      transform: scale(0.92);
    }
    to {
      opacity: 1;
      transform: scale(1.05);
    }
  }

  @keyframes onboarding-pop {
    from {
      opacity: 0;
      transform: scale(0.8);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }

  .onboarding-feat {
    display: flex;
    align-items: flex-start;
    gap: 0.5rem;
    border-radius: calc(var(--radius) - 2px);
    border: 1px solid hsl(var(--border) / 0.6);
    background: hsl(var(--muted) / 0.4);
    padding: 0.75rem;
    animation: onboarding-rise var(--motion-slow) var(--ease-apple) both;
    animation-delay: var(--feat-delay, 0ms);
  }

  @keyframes onboarding-rise {
    from {
      opacity: 0;
      transform: translateY(10px);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }

  /* CTA breathing glow. The class lands on the shared Button's element, so
     it is targeted globally and scoped by its onboarding-specific name. */
  :global(.onboarding-cta-pulse) {
    animation: onboarding-cta 2.6s var(--ease-standard) infinite;
  }

  @keyframes onboarding-cta {
    0%,
    100% {
      box-shadow: var(--shadow-primary-md);
    }
    50% {
      box-shadow: var(--shadow-primary-lg), 0 0 0 6px hsl(var(--primary) / 0.1);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .onboarding-halo,
    .onboarding-logo,
    .onboarding-feat,
    :global(.onboarding-cta-pulse) {
      animation: none;
    }
  }
</style>
