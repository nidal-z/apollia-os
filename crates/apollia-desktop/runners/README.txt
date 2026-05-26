Apollia OS — runners staging directory (ADR-113)

Ce dossier est rempli par scripts/bundle-cli.sh pendant le bundle Tauri.
Les binaires `apollia-runner-{cpu,metal,cuda,rocm,vulkan}[.exe]` sont copiés
ici avant que Tauri n'assemble le .dmg / .deb / .msi.

Ce fichier README sert de sentinelle pour que le glob `runners/**` matche
même quand aucun runner n'a encore été buildé (cargo check pur, dev local
sans bundle complet).
