// Fonts are bundled, never fetched. The families declared in
// tailwind.config.ts must all resolve offline: a remote <link> in index.html
// would make a fresh launch reach a third party before the user clicks
// anything, which the sovereignty guarantee forbids. The CSP in
// tauri.conf.json blocks the regression at the webview level.
import "@fontsource/inter-tight/300.css";
import "@fontsource/inter-tight/400.css";
import "@fontsource/inter-tight/400-italic.css";
import "@fontsource/inter-tight/500.css";
import "@fontsource/inter-tight/600.css";
import "@fontsource/inter-tight/700.css";
import "@fontsource/inter/400.css";
import "@fontsource/inter/500.css";
import "@fontsource/inter/600.css";
import "@fontsource/inter/700.css";
import "@fontsource/jetbrains-mono/400.css";
import "@fontsource/jetbrains-mono/500.css";
import "./app.css";
import "$lib/i18n";
import App from "./App.svelte";
import { mount } from "svelte";

const app = mount(App, {
  target: document.getElementById("app")!,
});

export default app;
