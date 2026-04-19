<script lang="ts">
  import { Tooltip } from "$lib/components/ui/tooltip";

  interface Props {
    skillId: string;
    skillName: string;
    description?: string | null;
    agentName?: string | null;
  }

  let { skillId, skillName, description = null, agentName = null }: Props = $props();

  const palette = [
    { bg: "bg-primary/15", text: "text-primary", ring: "ring-primary/20" },
    { bg: "bg-secondary/15", text: "text-secondary", ring: "ring-secondary/20" },
    { bg: "bg-success/15", text: "text-success", ring: "ring-success/20" },
    { bg: "bg-warning/15", text: "text-warning", ring: "ring-warning/20" },
    { bg: "bg-accent/15", text: "text-accent", ring: "ring-accent/20" },
    { bg: "bg-info/15", text: "text-info", ring: "ring-info/20" },
  ];

  function hash(str: string): number {
    let h = 0;
    for (let i = 0; i < str.length; i++) h = (h * 31 + str.charCodeAt(i)) | 0;
    return Math.abs(h);
  }

  const color = $derived(palette[hash(skillId) % palette.length]!);
  const tooltipContent = $derived(
    [
      agentName ? `${agentName} · ${skillName}` : skillName,
      description ? `\n${description}` : "",
      `\nskill_id: ${skillId}`,
    ].join(""),
  );
</script>

<Tooltip content={tooltipContent} side="top" delay={150}>
  <span
    class="inline-flex items-center rounded-full px-2 py-0.5 text-[10px] font-medium ring-1 ring-inset {color.bg} {color.text} {color.ring}"
    data-testid="a2a-skill-chip-{skillId}"
  >
    {skillName}
  </span>
</Tooltip>
