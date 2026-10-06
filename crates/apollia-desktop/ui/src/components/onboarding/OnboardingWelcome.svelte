<script lang="ts">
  /**
   * First-launch step 1 - the language of the interface.
   *
   * A welcome in two sentences and the choice of the language, made here
   * because every step after this one is read in it. Nothing technical is
   * said or asked: the step names no engine, no provider and no duration.
   *
   * Designed to fit inside the shared {@link OnboardingModal} overlay (no own
   * backdrop or fixed positioning). "Continue" is never disabled: a language
   * is always selected, so there is nothing to pass.
   */
  import { Button } from "$lib/components/ui/button";
  import { t, locale } from "svelte-i18n";
  import { setLocale, SUPPORTED_LOCALES } from "$lib/i18n";

  interface Props {
    onnext: () => void;
  }

  const { onnext }: Props = $props();

  /** The name of each language, the one the settings show for the same choice. */
  const LOCALE_NAME_KEYS: Record<(typeof SUPPORTED_LOCALES)[number], string> = {
    fr: "settings.language_fr",
    en: "settings.language_en",
  };

  function changeLocale(next: string): void {
    setLocale(next);
    locale.set(next);
  }
</script>

<div class="flex flex-col gap-5 px-1 py-2" data-testid="onboarding-welcome">
  <div class="flex items-center gap-4">
    <div class="onboarding-mark" aria-hidden="true">
      <span class="onboarding-halo"></span>
      <img src="/logo.svg" alt="" class="onboarding-logo" />
    </div>
    <div class="space-y-1.5">
      <h2 class="text-display-sm text-foreground">
        {$t("onboarding_welcome.title")}
      </h2>
      <p class="max-w-xl text-body-sm leading-relaxed text-muted-foreground">
        {$t("onboarding_welcome.tagline")}
      </p>
    </div>
  </div>

  <div class="space-y-2">
    <div
      class="inline-flex items-center gap-1 rounded-lg border border-border/60 bg-muted p-1"
      role="group"
      aria-label={$t("onboarding_welcome.language_label")}
      data-testid="onboarding-lang-toggle"
    >
      {#each SUPPORTED_LOCALES as code}
        <button
          type="button"
          class="rounded-md px-3 py-1 text-body-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary {$locale ===
          code
            ? 'bg-card text-foreground shadow-sm'
            : 'text-muted-foreground hover:text-foreground'}"
          onclick={() => changeLocale(code)}
          aria-pressed={$locale === code}
          data-testid={`onboarding-lang-${code}`}
        >
          {$t(LOCALE_NAME_KEYS[code])}
        </button>
      {/each}
    </div>
    <p class="text-caption text-muted-foreground">
      {$t("onboarding_welcome.language_hint")}
    </p>
  </div>

  <footer class="flex justify-end border-t border-border/70 pt-4">
    <Button
      variant="primary-gradient"
      size="default"
      onclick={onnext}
      data-testid="onboarding-welcome-cta"
    >
      {$t("common.continue")}
    </Button>
  </footer>
</div>

<style>
  .onboarding-mark {
    position: relative;
    width: 3.5rem;
    height: 3.5rem;
    flex-shrink: 0;
    display: grid;
    place-items: center;
  }

  .onboarding-halo {
    position: absolute;
    inset: -0.75rem;
    border-radius: 50%;
    background: radial-gradient(circle, hsl(var(--grad-b) / 0.45), transparent 68%);
    animation: onboarding-halo 2.4s var(--ease-standard) infinite alternate;
  }

  .onboarding-logo {
    position: relative;
    width: 3.5rem;
    height: 3.5rem;
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

  @media (prefers-reduced-motion: reduce) {
    .onboarding-halo,
    .onboarding-logo {
      animation: none;
    }
  }
</style>
