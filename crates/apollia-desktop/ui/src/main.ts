// Fonts are bundled, never fetched. The families declared in
// tailwind.config.ts must all resolve offline: a remote <link> in index.html
// would make a fresh launch reach a third party before the user clicks
// anything, which the sovereignty guarantee forbids. The CSP in
// tauri.conf.json blocks the regression at the webview level.
//
// Only the latin and latin-ext subsets are imported. The weight-level entry
// points (`400.css`) pull cyrillic, cyrillic-ext, greek, greek-ext and
// vietnamese as well, five alphabets that a two-locale catalogue, en.json and
// fr.json, has no text to render. The subset entry points ship the same faces
// for the text the product actually paints.
import "@fontsource/inter-tight/latin-300.css";
import "@fontsource/inter-tight/latin-400.css";
import "@fontsource/inter-tight/latin-400-italic.css";
import "@fontsource/inter-tight/latin-500.css";
import "@fontsource/inter-tight/latin-600.css";
import "@fontsource/inter-tight/latin-700.css";
import "@fontsource/inter-tight/latin-ext-300.css";
import "@fontsource/inter-tight/latin-ext-400.css";
import "@fontsource/inter-tight/latin-ext-400-italic.css";
import "@fontsource/inter-tight/latin-ext-500.css";
import "@fontsource/inter-tight/latin-ext-600.css";
import "@fontsource/inter-tight/latin-ext-700.css";
import "@fontsource/jetbrains-mono/latin-400.css";
import "@fontsource/jetbrains-mono/latin-500.css";
import "@fontsource/jetbrains-mono/latin-ext-400.css";
import "@fontsource/jetbrains-mono/latin-ext-500.css";
import "./app.css";
import "$lib/i18n";
import App from "./App.svelte";
import { mount } from "svelte";

const app = mount(App, {
  target: document.getElementById("app")!,
});

export default app;
