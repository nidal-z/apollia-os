<script lang="ts">
  import { Search } from "lucide-svelte";
  import { t } from "svelte-i18n";
  import { Input } from "$lib/components/ui/input";

  interface Props {
    value: string;
    placeholder?: string;
    onchange: (value: string) => void;
  }

  let { value, placeholder, onchange }: Props = $props();

  const resolvedPlaceholder = $derived(placeholder ?? $t("integrations.catalogue.search_placeholder"));
</script>

<div class="relative" data-testid="catalogue-search-bar">
  <Search
    size={15}
    class="absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground pointer-events-none"
    aria-hidden="true"
  />
  <Input
    type="search"
    class="pl-9"
    {value}
    placeholder={resolvedPlaceholder}
    oninput={(e) => onchange((e.currentTarget as HTMLInputElement).value)}
    data-testid="catalogue-search-input"
    aria-label={resolvedPlaceholder}
  />
</div>
