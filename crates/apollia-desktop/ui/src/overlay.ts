import RecordingOverlay from "./components/stt/RecordingOverlay.svelte";
import { mount } from "svelte";

const overlay = mount(RecordingOverlay, {
  target: document.getElementById("overlay")!,
});

export default overlay;
