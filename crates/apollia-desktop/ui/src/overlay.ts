import "$lib/i18n";
import { waitLocale } from "svelte-i18n";
import RecordingOverlay from "./components/stt/RecordingOverlay.svelte";
import { mount } from "svelte";

await waitLocale();
mount(RecordingOverlay, {
  target: document.getElementById("overlay")!,
});
