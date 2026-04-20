<script lang="ts">
  /**
   * Coloured badge for a template category / difficulty / source chip
   * (US-SP42-058).
   *
   * Keeps the palette centralised so the detail sheet and the card stay
   * visually consistent.
   */
  import { Badge } from "$lib/components/ui/badge";
  import type { TemplateCategory, TemplateDifficulty, TemplateSource } from "$lib/templates/registry";

  type BadgeKind = "category" | "difficulty" | "source";

  interface Props {
    kind: BadgeKind;
    value: TemplateCategory | TemplateDifficulty | TemplateSource;
    label: string;
  }

  let { kind, value, label }: Props = $props();

  const variant = $derived.by<"default" | "secondary" | "outline" | "destructive">(() => {
    if (kind === "source") return value === "official" ? "default" : "outline";
    if (kind === "difficulty") {
      if (value === "simple") return "secondary";
      if (value === "intermediate") return "outline";
      return "destructive";
    }
    return "secondary";
  });
</script>

<Badge
  variant={variant}
  class="text-[10px] px-1.5 py-0"
  data-testid="template-badge-{kind}-{value}"
>
  {label}
</Badge>
