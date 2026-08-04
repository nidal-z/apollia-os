<script lang="ts" context="module">
  export const meta = {
    title: "settings.nav.help",
    icon: "life-buoy",
    group: "settings.nav.cluster_help",
    cluster: "help",
  } as const;
</script>

<script lang="ts">
  import { t } from "svelte-i18n";
  import { BookOpen } from "lucide-svelte";
  import { Card } from "$lib/components/ui/card";
  import SettingsSubPage from "../../components/settings/SettingsSubPage.svelte";
  import HelpHero from "../../components/settings/help/HelpHero.svelte";
  import HelpTopics from "../../components/settings/help/HelpTopics.svelte";
  import HelpResources from "../../components/settings/help/HelpResources.svelte";
  import HelpFaq from "../../components/settings/help/HelpFaq.svelte";

  // The hero CTA opens the operator help center, published from docs/site by
  // CI. This page is a map, not a copy: it points at the published guides
  // rather than duplicating them, so the two never drift apart. No count here
  // on purpose, the corpus grows and a number in a comment goes stale silently.
  const HELP_CENTER_URL = "https://docs.apollia.fr/operator-help";

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

<SettingsSubPage route="help" data-testid="help-section">
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
</SettingsSubPage>
