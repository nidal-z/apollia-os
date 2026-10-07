<script lang="ts">
  import { Sheet } from "$lib/components/ui/sheet";
  import type { ChatSessionDetail } from "$lib/types";
  import ChatSessionPanel from "./ChatSessionPanel.svelte";

  interface Props {
    open: boolean;
    session: ChatSessionDetail | null;
    onclose: () => void;
    onupdated: () => void;
  }

  let { open, session, onclose, onupdated }: Props = $props();
</script>

<Sheet {open} {onclose}>
  <ChatSessionPanel {session} {onupdated} />
</Sheet>
