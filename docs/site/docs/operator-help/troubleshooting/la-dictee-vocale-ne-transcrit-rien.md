# La dictée vocale ne transcrit rien

> Pour tout operator qui appuie sur son raccourci de dictée sans voir apparaître de texte : retrouver une transcription fluide en quelques minutes.

## Vérifications rapides (par ordre de probabilité)

### 1. Le raccourci clavier n'est pas reconnu

Le raccourci de dictée peut entrer en conflit avec un raccourci système (Spotlight, capture d'écran) ou avec une autre application.

**Solution :**
1. Dans la sidebar, cliquez sur **Paramètres**, puis sur la section **Reconnaissance vocale**.
2. Repérez la ligne **Raccourci global** : la combinaison actuelle s'affiche sous forme de touches stylisées.
3. **Cliquez sur la combinaison** : un dialog plein écran de capture s'ouvre. Appuyez sur la nouvelle combinaison souhaitée - elle est capturée en temps réel et enregistrée à la validation.
   `[SCREENSHOT: dialog plein écran de capture de raccourci, message "Appuyez sur la combinaison souhaitée", touches détectées en gros caractères]`
4. Quittez le dialog avec **Échap** pour annuler.
5. Testez à nouveau le raccourci : **un overlay d'enregistrement avec visualizer audio** doit s'afficher dès l'appui.

### 2. Le modèle de transcription n'est pas téléchargé

Apollia transcrit en local avec un modèle Whisper. Sans modèle chargé, l'appui sur le raccourci ne produit rien.

**Solution :**
1. Ouvrez **Paramètres → Reconnaissance vocale** et regardez l'état du moteur en haut de la page.
2. Si l'état affiche *« Modèle non chargé »* ou équivalent, ouvrez **Paramètres → Hub de modèles** et téléchargez au minimum le modèle **Whisper Small** (suffisant pour le français).
3. Revenez sur la page **Reconnaissance vocale** : l'état doit afficher **Modèle chargé**.

### 3. Votre microphone système est muet ou mal sélectionné

Si le micro est désactivé au niveau du système d'exploitation, Apollia n'entend rien - sans message d'erreur.

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
1. Dans **Paramètres → Reconnaissance vocale**, vérifiez le champ **Langue**.
2. Sélectionnez **Français** (ou la langue effective de votre dictée).
3. Refaites un essai court de quelques secondes.

## Si rien ne fonctionne

1. Allez dans **Transcriptions** *(visible dans la sidebar en mode Builder)* pour voir si des essais récents ont produit du contenu vide ou incohérent : cela aide à localiser le problème.
2. Téléchargez un modèle Whisper plus précis (**Medium** ou **Large**) depuis **Paramètres → Hub de modèles** si vos dictées sont systématiquement floues.
3. Relancez Apollia après chaque changement de modèle ou de raccourci pour que le moteur recharge sa configuration.

> **Référence technique :** [Référence Apollia](../../reference/index.md) - comprendre comment Apollia capte, traite et stocke vos dictées en local.
