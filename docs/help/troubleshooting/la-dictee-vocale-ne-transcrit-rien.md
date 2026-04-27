# La dictée vocale ne transcrit rien

> Pour tout operator qui appuie sur son raccourci de dictée sans voir apparaître de texte : retrouver une transcription fluide en quelques minutes.

## Vérifications rapides (par ordre de probabilité)

### 1. Le raccourci clavier n'est pas reconnu

Le raccourci de dictée peut entrer en conflit avec un raccourci système (Spotlight, capture d'écran) ou avec une autre application.

**Solution :**
1. Dans la sidebar, cliquez sur **Settings**, puis sur l'onglet **Dictée vocale**.
2. Repérez le champ **Raccourci** et appuyez sur la combinaison souhaitée pour vérifier qu'elle est bien capturée.
   `[SCREENSHOT: page Settings Dictée vocale, champ Raccourci en focus avec combinaison affichée]`
3. Si elle ne s'affiche pas, choisissez une combinaison différente (par exemple `Cmd+Shift+D`) et enregistrez.
4. Testez à nouveau : un indicateur rouge **Enregistrement** doit apparaître dès l'appui.

### 2. Le modèle de transcription n'est pas téléchargé

Apollia transcrit en local avec le modèle Whisper. Sans modèle chargé, l'appui sur le raccourci ne produit rien.

**Solution :**
1. Ouvrez **Settings → Dictée vocale** et regardez l'état du modèle en haut de la page.
2. Si le modèle est marqué **Non téléchargé**, cliquez sur **Settings → Model Hub** et téléchargez au minimum le modèle **Whisper Small** (suffisant pour le français).
3. Revenez sur la page **Dictée vocale** : l'état doit afficher **Chargé**.

### 3. Votre microphone système est muet ou mal sélectionné

Si le micro est désactivé au niveau du système d'exploitation, Apollia n'entend rien — sans message d'erreur.

**Solution :**
1. Ouvrez les réglages son de votre système et vérifiez que le bon microphone est sélectionné comme entrée par défaut.
2. Parlez normalement : le niveau d'entrée doit bouger.
3. Si rien ne bouge, débranchez et rebranchez le micro (ou ajustez le volume d'entrée).

### 4. Apollia n'a pas l'autorisation d'utiliser le microphone

Au premier usage, le système demande la permission d'accès au micro. Si elle a été refusée, Apollia reste muette.

**Solution :**
1. Dans les réglages **Confidentialité** de votre système, ouvrez la section **Microphone**.
2. Vérifiez qu'**Apollia** figure dans la liste des applications autorisées et que la case est cochée.
3. Si elle ne l'est pas, cochez-la, puis relancez Apollia.

### 5. La langue de transcription ne correspond pas

Le modèle Whisper transcrit selon la langue configurée. Une langue erronée produit du texte incohérent ou rien d'utile.

**Solution :**
1. Dans **Settings → Dictée vocale**, vérifiez le champ **Langue**.
2. Sélectionnez **Français** (ou la langue effective de votre dictée).
3. Refaites un essai court de quelques secondes.

## Si rien ne fonctionne

1. Allez dans **Transcriptions** pour voir si des essais récents ont produit du contenu vide ou incohérent : cela aide à localiser le problème.
2. Téléchargez un modèle Whisper plus précis (**Medium** ou **Large**) depuis **Model Hub** si vos dictées sont systématiquement floues.
3. Relancez Apollia après chaque changement de modèle ou de raccourci.

> **Concept :** [Briques-STT](https://github.com/nidal-z/apollia-os/wiki/Briques-STT) — comprendre comment Apollia capte, traite et stocke vos dictées en local.
