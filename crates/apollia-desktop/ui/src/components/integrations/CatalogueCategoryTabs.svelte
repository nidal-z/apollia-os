<script lang="ts">
  import { t } from "svelte-i18n";
  import { TabBar } from "$lib/components/ui/tabs";

  interface Props {
    categories: string[];
    selected: string;
    onchange: (category: string) => void;
  }

  let { categories, selected, onchange }: Props = $props();

  /** Resolves the i18n label for a category, with capitalized fallback. */
  function labelFor(cat: string): string {
    if (cat === "all") return $t("integrations.catalogue.category_all");
    const key = `integrations.categories.${cat}`;
    const translated = $t(key);
    // If the i18n key is missing, $t returns the key itself — fall back to capitalized.
    if (translated === key) return cat.charAt(0).toUpperCase() + cat.slice(1);
    return translated;
  }

  const tabItems = $derived(
    categories.map((cat) => ({ key: cat, label: labelFor(cat) })),
  );
</script>

<TabBar
  items={tabItems}
  activeTab={selected}
  ontabchange={onchange}
  testidPrefix="catalogue-category"
/>
