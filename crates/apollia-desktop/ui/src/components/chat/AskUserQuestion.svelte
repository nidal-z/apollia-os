<script lang="ts">
  import { Input } from "$lib/components/ui/input";
  import { Textarea } from "$lib/components/ui/textarea";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { RadioGroup, RadioItem } from "$lib/components/ui/radio";

  interface UserQuestion {
    id: string;
    question: string;
    type: "open" | "single_choice" | "multi_choice";
    options?: string[];
    hint?: string;
  }

  interface Props {
    question: UserQuestion;
    index: number;
    disabled?: boolean;
    value: string;
    selectedValues: string[];
    onvaluechange: (value: string) => void;
    onselectedchange: (values: string[]) => void;
  }

  let {
    question,
    index,
    disabled = false,
    value = "",
    selectedValues = [],
    onvaluechange,
    onselectedchange,
  }: Props = $props();

  function handleCheckboxToggle(option: string, checked: boolean): void {
    const updated = checked
      ? [...selectedValues, option]
      : selectedValues.filter((v) => v !== option);
    onselectedchange(updated);
  }
</script>

<div class="mt-3 first:mt-2" data-testid="ask-user-question-{question.id}">
  <label
    for="ask-user-{question.id}"
    class="flex items-baseline gap-1.5 text-[12px] font-medium text-foreground"
  >
    <span class="text-muted-foreground">{index + 1}.</span>
    {question.question}
  </label>

  <div class="mt-1.5">
    {#if question.type === "open"}
      {#if question.hint && question.hint.length > 60}
        <Textarea
          id="ask-user-{question.id}"
          {disabled}
          placeholder={question.hint ?? ""}
          value={value}
          oninput={(e) => onvaluechange(e.currentTarget.value)}
        />
      {:else}
        <Input
          id="ask-user-{question.id}"
          {disabled}
          placeholder={question.hint ?? ""}
          value={value}
          oninput={(e) => onvaluechange(e.currentTarget.value)}
        />
      {/if}

    {:else if question.type === "single_choice" && question.options}
      <RadioGroup
        value={value}
        onchange={(v) => onvaluechange(v)}
      >
        {#each question.options as option}
          <label
            class="flex items-center gap-2 rounded-md px-2 py-1.5 cursor-pointer
                   hover:glass-inset transition-colors
                   {disabled ? 'opacity-50 cursor-not-allowed' : ''}"
          >
            <RadioItem value={option} {disabled} />
            <span class="text-[12px] text-foreground">{option}</span>
          </label>
        {/each}
      </RadioGroup>

    {:else if question.type === "multi_choice" && question.options}
      <div class="flex flex-col gap-1" role="group" aria-label={question.question}>
        {#each question.options as option}
          <label
            class="flex items-center gap-2 rounded-md px-2 py-1.5 cursor-pointer
                   hover:glass-inset transition-colors
                   {disabled ? 'opacity-50 cursor-not-allowed' : ''}"
          >
            <Checkbox
              checked={selectedValues.includes(option)}
              onchange={(checked) => handleCheckboxToggle(option, checked)}
              {disabled}
            />
            <span class="text-[12px] text-foreground">{option}</span>
          </label>
        {/each}
      </div>
    {/if}
  </div>
</div>
