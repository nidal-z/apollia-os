<script lang="ts">
  interface Props {
    text: string;
  }

  let { text }: Props = $props();
</script>

<span class="inline" data-testid="chat-streaming-text">
  {text}<span class="inline-block h-4 w-[2px] animate-pulse bg-primary align-text-bottom"></span>
</span>
