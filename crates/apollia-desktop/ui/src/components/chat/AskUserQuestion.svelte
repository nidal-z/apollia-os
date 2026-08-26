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
    class="flex items-baseline gap-1.5 text-body-xs font-medium text-foreground"
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
      <RadioGroup value={value}>
        {#each question.options as option}
          <div
            class="flex items-center rounded-md px-2 py-1.5
                   hover:glass-inset transition-colors
                   {disabled ? 'opacity-50' : ''}"
          >
            <RadioItem
              value={option}
              checked={value === option}
              onchange={(v) => onvaluechange(v)}
              {disabled}
              class="gap-2"
            >
              <span class="text-body-xs text-foreground">{option}</span>
            </RadioItem>
          </div>
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
            <span class="text-body-xs text-foreground">{option}</span>
          </label>
        {/each}
      </div>
    {/if}
  </div>
</div>
