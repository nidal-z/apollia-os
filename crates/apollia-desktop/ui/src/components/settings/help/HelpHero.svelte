<script lang="ts">
  /**
   * HelpHero - the signature getting-started card of the Help sub-page.
   *
   * The single "vitrine" surface of an otherwise calm Operator page: the
   * signature gradient shows up only here (two low-alpha radial washes, a
   * vertical gradient rule, and the gradient icon tile). Every other card on
   * the page stays a neutral surface. All colour comes from design tokens.
   *
   * The CTA is a real `<a>` routed through the system opener so the URL leaves
   * Apollia in the default browser, never in-app. When `disabled` (the machine
   * is offline) the CTA is inert but stays visible, never hidden.
   */
  import { LifeBuoy, ExternalLink } from "lucide-svelte";
  import { handleExternalLinkClick } from "$lib/utils/externalLink";

  interface Props {
    title: string;
    body: string;
    cta: string;
    href: string;
    disabled?: boolean;
  }

  let { title, body, cta, href, disabled = false }: Props = $props();
</script>

<div
  class="help-hero relative flex flex-col items-start gap-4 overflow-hidden rounded-xl border
    border-[hsl(var(--primary-gradient-to)/0.3)] p-5 shadow-primary-lg
    sm:flex-row sm:items-center"
  data-testid="help-hero"
>
  <span
    class="help-hero-bar pointer-events-none absolute inset-y-0 left-0 w-1"
    aria-hidden="true"
  ></span>

  <span
    class="grid h-11 w-11 shrink-0 place-items-center rounded-lg bg-primary-gradient
      text-primary-foreground shadow-primary-md"
  >
    <LifeBuoy size={22} strokeWidth={1.75} aria-hidden="true" />
  </span>

  <div class="min-w-0 flex-1">
    <h2 class="text-heading-sm text-foreground">{title}</h2>
    <p class="mt-1 max-w-md text-body-sm text-muted-foreground">{body}</p>
  </div>

  <a
    {href}
    rel="noreferrer"
    onclick={handleExternalLinkClick}
    aria-disabled={disabled}
    tabindex={disabled ? -1 : 0}
    class="inline-flex shrink-0 items-center gap-2 rounded-md bg-primary px-4 py-2
      text-body-sm font-semibold text-primary-foreground shadow-primary-md
      transition hover:bg-primary-hover hover:shadow-primary-lg
      focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/50
      motion-reduce:transition-none
      aria-disabled:pointer-events-none aria-disabled:opacity-50"
    data-testid="help-hero-cta"
  >
    {cta}
    <ExternalLink size={14} strokeWidth={2} aria-hidden="true" />
  </a>
</div>

<style>
  /* Signature gradient, tokens only: two low-alpha radial washes over the card
     surface. This is the one place on the page the brand gradient appears. */
  .help-hero {
    background:
      radial-gradient(120% 140% at 8% 0%, hsl(var(--grad-a) / 0.16), transparent 55%),
      radial-gradient(120% 160% at 100% 100%, hsl(var(--grad-b) / 0.18), transparent 60%),
      hsl(var(--card));
  }
  .help-hero-bar {
    background: linear-gradient(180deg, hsl(var(--grad-a)), hsl(var(--grad-b)));
  }
</style>
