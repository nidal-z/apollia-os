// Entry of the window the logs of the local server detach into
// (`logs.html`, opened by the `open_server_logs_window` command). The fonts
// are bundled like the main window's: nothing is fetched.
import "@fontsource/inter-tight/latin-400.css";
import "@fontsource/inter-tight/latin-500.css";
import "@fontsource/inter-tight/latin-600.css";
import "@fontsource/inter-tight/latin-ext-400.css";
import "@fontsource/inter-tight/latin-ext-500.css";
import "@fontsource/inter-tight/latin-ext-600.css";
import "@fontsource/jetbrains-mono/latin-400.css";
import "@fontsource/jetbrains-mono/latin-ext-400.css";
import "./app.css";
import "$lib/i18n";
import { locale, waitLocale } from "svelte-i18n";
import { mount } from "svelte";
import { initTheme } from "$lib/stores/theme";
import LogsWindow from "./routes/LogsWindow.svelte";

locale.subscribe((value) => {
  if (value) document.documentElement.lang = value.split("-")[0];
});

initTheme();
await waitLocale();
mount(LogsWindow, {
  target: document.getElementById("logs")!,
});
