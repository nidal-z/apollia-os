<script lang="ts">
  /**
   * AboutHero - the vitrine moment of the settings hub.
   *
   * The one surface where the indigo->violet signature gradient is fully
   * deployed: a radial wash background, the Apollia symbol, and a gradient
   * text wordmark with a slow shimmer. Version / channel / platform pills draw
   * their values from the live systemInfo store. Every value composes an
   * existing HSL token, so both themes and reduced-motion are honored.
   *
   * The mark is the real logo, the same `/logo.svg` the sidebar, the onboarding
   * welcome and the companion header already serve. It is a light-background
   * artwork: measured against the dark theme, its dominant blue swoosh lands at
   * 1.36:1 on `--card`, well under the 3:1 floor WCAG 1.4.11 sets for a
   * non-text graphic, and the body of the mark disappears. There is no dark
   * variant of the vector, so at 64 px it is seated on `.logo-plaque`, the
   * paper it was drawn against, which holds every ink above 4.2:1 in both
   * themes. Flat paper, not a gradient tile: two gradients at this size fight.
   */
  import { t } from "svelte-i18n";

  interface Props {
    version: string | null;
    os: string | null;
    /** Already-translated channel label ("Stable" / "Preview"), or null. */
    channelLabel: string | null;
  }

  let { version, os, channelLabel }: Props = $props();
</script>

<div
  class="hero-frame relative overflow-hidden rounded-xl border p-6 shadow-elev-2 animate-fade-in"
  data-testid="about-hero"
>
  <div class="flex items-center gap-4">
    <span class="logo-plaque hero-mark grid h-16 w-16 shrink-0 place-items-center rounded-2xl">
      <img src="/logo.svg" alt="" width="48" height="48" class="h-12 w-12" data-testid="about-logo" />
    </span>
    <div class="min-w-0">
      <h2 class="hero-wordmark m-0 font-sans text-display-sm">Apollia OS</h2>
      <p class="mt-1 max-w-prose text-body-sm text-muted-foreground">
        {$t("settings.about.tagline")}
      </p>
    </div>
  </div>

  <div class="mt-4 flex flex-wrap items-center gap-2">
    {#if version}
      <span
        class="inline-flex items-center rounded-full border border-border bg-surface-1/70 px-2.5 py-0.5 font-mono text-caption text-foreground"
        data-testid="about-version"
      >
        v{version}
      </span>
    {:else}
      <span
        class="inline-flex items-center rounded-full border border-border bg-surface-1/70 px-2.5 py-0.5 text-caption text-muted-foreground"
      >
        {$t("settings.about.version_unknown")}
      </span>
    {/if}

    {#if channelLabel}
      <span class="channel-pill inline-flex items-center gap-1.5 rounded-full border px-2.5 py-0.5 text-caption font-medium">
        <span class="channel-dot h-1.5 w-1.5 rounded-full" aria-hidden="true"></span>
        {channelLabel}
      </span>
    {/if}

    {#if os}
      <span
        class="inline-flex items-center rounded-full border border-border bg-surface-1/50 px-2.5 py-0.5 font-mono text-caption text-muted-foreground"
        data-testid="about-os"
      >
        {os}
      </span>
    {/if}
  </div>
</div>

<style>
  .hero-frame {
    border-color: hsl(var(--grad-b) / 0.28);
    background:
      radial-gradient(120% 140% at 84% -10%, hsl(var(--grad-b) / 0.18), transparent 46%),
      radial-gradient(120% 130% at 12% 0%, hsl(var(--grad-a) / 0.16), transparent 52%),
      linear-gradient(180deg, hsl(var(--card)), hsl(var(--surface-1)));
  }

  .hero-mark {
    filter: drop-shadow(0 8px 20px hsl(var(--grad-a) / 0.32));
  }

  .hero-mark img {
    object-fit: contain;
  }

  .hero-wordmark {
    background-image: linear-gradient(
      100deg,
      hsl(var(--grad-a)),
      hsl(var(--grad-b)) 40%,
      hsl(var(--grad-a)) 80%,
      hsl(var(--grad-b))
    );
    background-size: 220% 100%;
    background-position: 0% 50%;
    -webkit-background-clip: text;
    background-clip: text;
    color: transparent;
    -webkit-text-fill-color: transparent;
    animation: about-wordmark-shimmer 7s cubic-bezier(0.22, 1, 0.36, 1) infinite;
  }

  .channel-pill {
    border-color: hsl(var(--grad-b) / 0.4);
    background: hsl(var(--grad-b) / 0.1);
    color: hsl(var(--grad-b));
  }

  .channel-dot {
    background: hsl(var(--grad-b));
  }

  @keyframes about-wordmark-shimmer {
    0%,
    100% {
      background-position: 0% 50%;
    }
    50% {
      background-position: 100% 50%;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .hero-wordmark {
      animation: none;
      background-position: 0% 50%;
    }
  }
</style>
