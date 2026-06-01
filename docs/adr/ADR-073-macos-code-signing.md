# ADR-073 - Code signing macOS : ad-hoc pour v0.1.0, Developer ID en backlog

**Date :** 2026-04-17
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** Bloc 1.5 du LAUNCH-BACKLOG (packaging release v0.1.0, 27 avril 2026)

---

## Contexte

Apollia OS sort son v0.1.0 sous forme de DMG macOS universal2. Sans code signature,
macOS Gatekeeper applique le *quarantine bit* (`com.apple.quarantine` xattr) à tout
binaire téléchargé depuis Internet et **refuse l'exécution** avec le message
*« Apollia OS ne peut pas être ouverte car macOS ne peut pas vérifier qu'elle est exempte de logiciels malveillants »* - seuls les boutons « Jeter à la corbeille » ou « OK » sont proposés.

Cette friction est **rédhibitoire pour un prospect non-technique** qui reçoit un lien DM :
la majorité des utilisateurs abandonnent à ce stade sans connaître le workaround.

Trois options ont été évaluées :

| Option | Coût | UX téléchargement depuis Internet |
|---|---|---|
| **A. Pas de signature** | 0 € | Gatekeeper bloque complètement, message alarmant, pas de bouton « Ouvrir ». |
| **B. Ad-hoc signing** (`codesign -s -`) | 0 € | Même dialog alarmant, **mais clic-droit → Ouvrir** révèle un bouton « Ouvrir » qui whitelist l'app définitivement. Documentable. |
| **C. Apple Developer ID + notarize** | 99 USD/an + 2-3 h setup | Expérience native : double-clic → l'app s'ouvre directement. Pas de warning. |

Contrainte budgétaire pour v0.1.0 : **99 USD/an non soutenable** dans la phase actuelle du projet (solo, pré-revenu, 144 h de budget sur 12 jours calendaires).

---

## Décision

**Choix : Option B - ad-hoc signing avec hardened runtime et entitlements.**

Les deux binaires du bundle (`apollia-desktop`, `apollia-os`) sont signés en ad-hoc
(`codesign --force --deep --sign - --options runtime --entitlements entitlements.plist`)
lors du build CI, immédiatement après le packaging Tauri et avant la génération du DMG.

Le **hardened runtime est activé** même en ad-hoc, et un fichier `entitlements.plist`
accompagne la signature avec les exceptions minimales nécessaires au fonctionnement
de PyO3 avec un interpréteur Python bundled non-signé par Apple :

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTD PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>com.apple.security.cs.allow-unsigned-executable-memory</key>
    <true/>
    <key>com.apple.security.cs.allow-dyld-environment-variables</key>
    <true/>
    <key>com.apple.security.cs.disable-library-validation</key>
    <true/>
</dict>
</plist>
```

**Justification des entitlements :**
- `allow-unsigned-executable-memory` - PyO3 exécute du bytecode Python compilé dynamiquement.
- `allow-dyld-environment-variables` - `PYTHONHOME`/`PYTHONPATH`/`DYLD_LIBRARY_PATH` sont
  exportés au runtime par `setup_bundled_python()` avant `init_embedded()`. Sans cette
  entitlement, hardened runtime les purgerait silencieusement.
- `disable-library-validation` - l'app doit charger `libpython3.13.dylib` provenant du
  bundle `python-build-standalone` (signée par Astral mais pas par notre Team ID Apple,
  qu'on n'a pas). Sans cette entitlement, dyld refuserait le chargement.

### Configuration Tauri

`crates/apollia-desktop/tauri.conf.json` :

```json
"bundle": {
  "macOS": {
    "minimumSystemVersion": "13.0",
    "signingIdentity": "-",
    "hardenedRuntime": true,
    "entitlements": "entitlements.plist"
  }
}
```

`"signingIdentity": "-"` force Tauri à utiliser l'identité ad-hoc. La valeur sera
remplacée par la vraie identité Developer ID quand le Developer Program sera
souscrit (v0.2+).

### UX utilisateur documentée

Trois canaux publient la procédure de 1er lancement :

1. **README.md - section « Install »** - 2 étapes numérotées + GIF 10s :
   1. Télécharger le DMG, double-clic, glisser Apollia OS dans Applications.
   2. Au 1er lancement, **clic-droit sur l'icône → Ouvrir** → bouton « Ouvrir » dans le dialog.
2. **Landing page - bloc « Installation »** - même contenu avec screenshot.
3. **KNOWN-ISSUES.md - entrée « macOS 1st launch »** - workaround Terminal alternatif :
   `xattr -cr "/Applications/Apollia OS.app"`.

### Pipeline CI

`.github/workflows/build-desktop.yml` ajoute une étape post-build :

```bash
codesign --force --deep --sign - \
  --options runtime \
  --entitlements crates/apollia-desktop/entitlements.plist \
  "target/universal-apple-darwin/release/bundle/macos/Apollia OS.app"
```

Avant la génération du `.dmg` par Tauri, toute la hiérarchie (y compris le binaire `apollia-os` dans `Contents/Resources/` et la libpython dans `Contents/Resources/python/lib/`) est signée.

### Rejet de l'option A

Pas de signature = 0 chance de passer Gatekeeper sans intervention Terminal de l'utilisateur.
Crée une friction identique à B mais **sans bouton natif « Ouvrir »** dans le dialog - oblige
à sortir de l'app pour faire `xattr`. Inacceptable pour un prospect non-dev.

### Rejet de l'option C pour v0.1.0

99 USD/an non soutenable en phase pré-revenu. Décision à revisiter après le 27 avril
quand les premiers prospects auront généré un signal commercial.

---

## Conséquences

**Positives :**
- **Zéro coût** sortie v0.1.0.
- **Upgrade path propre** vers Developer ID : ajouter 6 secrets GitHub
  (`APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`,
  `APPLE_ID`, `APPLE_TEAM_ID`, `APPLE_PASSWORD`) et remplacer `"-"` par
  `"$APPLE_SIGNING_IDENTITY"` dans la config + ajouter une step `notarytool submit --wait`.
  Aucun changement dans le code applicatif ni dans les entitlements.
- **Hardened runtime actif** dès v0.1.0 - comportement runtime identique à celui qu'aura
  la version notarisée. Zéro surprise le jour de la migration.

**Négatives / Compromis :**
- **Warning Gatekeeper au 1er lancement** - ~30-50 % des prospects non-techniques peuvent
  abandonner à ce stade malgré la doc. Métrique à surveiller : nombre de téléchargements
  DMG vs nombre d'apps qui envoient un 1er event télémétrie (si on en ajoute).
- **Entitlement `disable-library-validation`** affaiblit le modèle de sécurité macOS.
  Acceptable car la dylib chargée (`libpython3.13.dylib` de python-build-standalone)
  est sourcée d'une organisation vérifiable (Astral) et sa somme SHA256 peut être
  auditée dans le workflow CI.

**Dette technique trackée :**
- Story future : *Passer à Developer ID Application + notarization*.
  Pré-requis : souscription Apple Developer Program (99 USD/an). Impact UX : suppression
  du warning 1er lancement. Coût dev : ~2-3 h (secrets CI + step notarytool).

---

## Principes architecturaux impactés

- **Principe #2 - Zéro dépendance externe :** Signature ad-hoc ne crée pas de dépendance
  à un service tiers (contrairement à notarize qui requiert `notarytool submit` à l'API
  Apple). Conforme.
- **Principe #4 - Fail fast :** Si `codesign` échoue dans le workflow CI, le build est
  rejeté avant la génération du DMG. Conforme.
- **Principe #8 - CLI humaine :** Le binaire `apollia-os` dans `Contents/Resources/`
  est signé ad-hoc aussi, assurant qu'une exécution en ligne de commande (après
  création du symlink via UI Settings) ne déclenche pas de prompt Gatekeeper séparé.
  Conforme.

---

## Liens

- `docs/internal/packaging-design.md` - conception globale du packaging v0.1.0 (§3.2).
- `LAUNCH-BACKLOG.md` - bloc 1.5 (items 1.5.1, 1.5.2, 1.5.8).
- Apple - [Code Signing Guide](https://developer.apple.com/library/archive/documentation/Security/Conceptual/CodeSigningGuide/Introduction/Introduction.html).
- Apple - [Hardened Runtime Entitlements](https://developer.apple.com/documentation/security/hardened_runtime).
- python-build-standalone - [Astral GitHub](https://github.com/astral-sh/python-build-standalone).
