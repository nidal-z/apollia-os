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
import { locale } from "svelte-i18n";
import App from "./App.svelte";
import { mount } from "svelte";
import { notifyUiLocale } from "$lib/ipc/app";

// `index.html` ships a static `lang="en"`, the value before the first render.
// The document language must follow the resolved locale (screen readers,
// hyphenation, spell checking), and the native shell needs it too for the
// tray menu and notifications. `locale` fires on subscription with the
// resolved initial value, then on every switch.
locale.subscribe((value) => {
  if (!value) return;
  const lang = value.split("-")[0];
  document.documentElement.lang = lang;
  void notifyUiLocale(lang);
});

const app = mount(App, {
  target: document.getElementById("app")!,
});

export default app;
