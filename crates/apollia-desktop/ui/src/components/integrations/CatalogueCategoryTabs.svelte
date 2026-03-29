<script lang="ts">
  import { t } from "svelte-i18n";
  import { TabBar } from "$lib/components/ui/tabs";

  interface Props {
    categories: string[];
    selected: string;
    onchange: (category: string) => void;
  }

  let { categories, selected, onchange }: Props = $props();

  /** Capitalizes the first letter of a category key for display. */
  function labelFor(cat: string): string {
    if (cat === "all") return $t("integrations.catalogue.category_all");
    return cat.charAt(0).toUpperCase() + cat.slice(1);
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
