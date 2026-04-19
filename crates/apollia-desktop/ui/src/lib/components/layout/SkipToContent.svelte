<script lang="ts">
  import { t } from "svelte-i18n";

  interface Props {
    /** id of the landmark to jump to. Defaults to `main-content`. */
    targetId?: string;
  }

  let { targetId = "main-content" }: Props = $props();

  function jump(event: MouseEvent) {
    event.preventDefault();
    const node = document.getElementById(targetId);
    if (!node) return;
    // Programmatically focus the landmark so the next Tab starts inside <main>.
    const previousTabindex = node.getAttribute("tabindex");
    if (previousTabindex === null) node.setAttribute("tabindex", "-1");
    node.focus({ preventScroll: false });
    if (previousTabindex === null) {
      node.addEventListener(
        "blur",
        () => node.removeAttribute("tabindex"),
        { once: true },
      );
    }
  }
</script>

<a
  class="skip-to-content"
  href={`#${targetId}`}
  onclick={jump}
  data-testid="skip-to-content"
>
  {$t("a11y.skip_to_content")}
</a>
