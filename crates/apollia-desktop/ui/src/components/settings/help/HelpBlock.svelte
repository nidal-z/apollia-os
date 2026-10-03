<script lang="ts">
  import { t, locale } from "svelte-i18n";
  import { BookOpen } from "lucide-svelte";
  import { Card } from "$lib/components/ui/card";
  import { docsUrlFor } from "$lib/utils/docsUrl";
  import HelpHero from "./HelpHero.svelte";
  import HelpTopics from "./HelpTopics.svelte";
  import HelpResources from "./HelpResources.svelte";
  import HelpFaq from "./HelpFaq.svelte";

  // The hero CTA opens the operator help center, published from docs/site by
  // CI. This page is a map, not a copy: it points at the published guides
  // rather than duplicating them, so the two never drift apart. No count here
  // on purpose, the corpus grows and a number in a comment goes stale silently.
  const HELP_CENTER_URL = $derived(docsUrlFor($locale, "/operator-help"));

  const faqIds = ["offline", "model", "data", "shortcuts"] as const;
  const faqItems = $derived(
    faqIds.map((id) => ({
      id,
      question: $t(`settings.help.faq_q_${id}`),
      answer: $t(`settings.help.faq_a_${id}`),
    })),
  );

  // External resources are disabled (not hidden) while offline; the hero CTA
  // and the offline banner track the same state.
  let online = $state(true);
  $effect(() => {
    online = navigator.onLine;
    const update = () => {
      online = navigator.onLine;
    };
    globalThis.addEventListener("online", update);
    globalThis.addEventListener("offline", update);
    return () => {
      globalThis.removeEventListener("online", update);
      globalThis.removeEventListener("offline", update);
    };
  });
</script>

<div class="flex flex-col gap-4" data-testid="help-section">
  <HelpHero
    title={$t("settings.help.hero_title")}
    body={$t("settings.help.hero_body")}
    cta={$t("settings.help.hero_cta")}
    href={HELP_CENTER_URL}
    disabled={!online}
  />

  <Card class="rounded-xl p-5" data-testid="help-intro">
    <div class="flex items-start gap-3">
      <BookOpen
        size={18}
        strokeWidth={1.75}
        class="mt-0.5 shrink-0 text-muted-foreground"
        aria-hidden="true"
      />
      <p class="text-body-sm leading-relaxed text-muted-foreground">{$t("settings.help.intro")}</p>
    </div>
  </Card>

  <HelpTopics />

  <HelpResources {online} />

  <section class="space-y-2.5" aria-label={$t("settings.help.faq_title")}>
    <h2 class="px-1 text-overline uppercase text-muted-foreground">
      {$t("settings.help.faq_title")}
    </h2>
    <HelpFaq items={faqItems} />
  </section>
</div>
