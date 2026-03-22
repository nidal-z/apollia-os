<script lang="ts">
  interface Props {
    text: string;
  }

  let { text }: Props = $props();
</script>

<span class="inline" data-testid="chat-streaming-text">
  {text}<span class="inline-block w-0.5 h-4 bg-primary animate-pulse align-text-bottom"></span>
</span>
