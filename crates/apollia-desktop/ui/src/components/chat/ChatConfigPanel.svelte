<script lang="ts">
  import { Sheet } from "$lib/components/ui/sheet";
  import type { ChatSessionDetail } from "$lib/types";
  import ChatConfigPanelBody from "./ChatConfigPanelBody.svelte";

  interface Props {
    open: boolean;
    session: ChatSessionDetail | null;
    onclose: () => void;
    onupdated: () => void;
  }

  let { open, session, onclose, onupdated }: Props = $props();
</script>

<Sheet {open} {onclose}>
  <ChatConfigPanelBody {session} {onupdated} {onclose} showHeader />
</Sheet>
