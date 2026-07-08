# Le fournisseur d'IA ne répond pas

> Pour tout operator dont le chat reste figé ou dont le **point d'état à gauche du mot *Apollia*** dans le bandeau supérieur passe à l'ambre ou au rouge : retrouver une IA fonctionnelle en moins de cinq minutes.

## Comprendre le point d'état Apollia

Apollia affiche **un seul indicateur global** d'état runtime + LLM, en haut à gauche, à côté du mot *Apollia* dans le breadcrumb :

- 🟢 **vert** - runtime sain et au moins un backend LLM prêt.
- 🟡 **ambre** - runtime sain mais aucun backend LLM connecté.
- 🔴 **rouge clignotant** - runtime déconnecté ou reconnexion en cours.

Survolez le point pour voir l'état exact (tooltip natif). Ce point est le repère central de cette page.

## Vérifications rapides (par ordre de probabilité)

### 1. Votre connexion internet est tombée

Les fournisseurs cloud (Anthropic, OpenAI, Vertex…) sont en ligne. Une coupure Wi-Fi ou VPN coupe le chat.

**Solution :**
1. Ouvrez un onglet de navigateur pour confirmer que vous êtes en ligne.
2. Une fois la connexion rétablie, attendez quelques secondes : le point d'état repasse au vert et vous pouvez relancer votre message.

### 2. La clé API du backend est invalide ou expirée

Une clé révoquée, expirée ou recopiée avec un espace en trop fait échouer toutes les requêtes.

**Solution :**
1. Dans la sidebar, ouvrez **Paramètres**, puis la section **Backends LLM**.
2. Repérez le backend marqué `✗ erreur` dans la liste. **Survolez le label de statut** : un tooltip natif affiche le motif d'erreur exact (ex. *« 401 Unauthorized »*, *« connection refused »*).
   `[SCREENSHOT: page Paramètres → Backends LLM, une carte de backend en erreur avec son icône XCircle rouge, tooltip natif au survol]`
3. Cliquez sur l'**icône Plug** (en première position dans les actions de la carte) pour re-tester la connexion. Un badge **OK · *Nms*** vert s'affiche en cas de succès, **Erreur** rouge sinon. Le badge s'efface après 5 secondes.
4. Si l'échec persiste, cliquez sur l'**icône crayon** pour ouvrir le dialog d'édition, collez une clé valide depuis la console du fournisseur, puis cliquez à nouveau sur **Tester la connexion** en bas du dialog.

### 3. Le nom du modèle est incorrect ou indisponible

Les fournisseurs renomment ou retirent régulièrement leurs modèles. Un identifiant obsolète provoque une erreur à chaque appel.

**Solution :**
1. Cliquez sur l'**icône crayon** de la carte en erreur.
2. Vérifiez le champ **Modèle** : il doit correspondre exactement à un identifiant valide chez le fournisseur. Consultez la documentation à jour du fournisseur - les identifiants évoluent au fil des mois.
3. Corrigez, cliquez sur **Tester la connexion** dans le dialog, puis sur **Enregistrer**.

### 4. Le service du fournisseur est en panne

Anthropic, OpenAI et les autres fournisseurs cloud publient des incidents sur leur page de statut publique. Si le test échoue alors que tout semble correct côté Apollia, l'origine est de leur côté.

**Solution :**
1. Consultez la page de statut du fournisseur concerné.
2. Si un incident est en cours, ajoutez un backend de secours (un autre fournisseur ou un modèle local) dans **Paramètres → Backends LLM** pour ne pas rester bloqué. Le routage choisira automatiquement un backend prêt.

### 5. Le service local n'est plus actif

Si vous utilisez un modèle local Apollia (llama.cpp) ou Ollama, le moteur doit tourner sur votre machine.

**Solution :**
1. Pour **Ollama**, vérifiez que le service Ollama est démarré sur votre poste.
2. Pour un **modèle local Apollia**, ouvrez **Paramètres → Hub de modèles** et confirmez que le modèle est bien chargé.
3. Cliquez sur l'**icône Plug** sur la carte du backend correspondant pour re-tester.

## Si rien ne fonctionne

1. **Quittez complètement Apollia et relancez** : la connexion au fournisseur est re-testée automatiquement au démarrage. Le point d'état repasse au vert si tout va bien.
2. Si le point reste rouge clignotant (runtime down) : c'est un problème côté runtime Apollia, pas LLM. Consultez `~/.apollia/logs/` (ou `Paramètres → Zone de danger → Effacer les logs` pour repartir d'un état propre).
3. Pour suivre **quand exactement** la perte a eu lieu, ouvrez **Boîte de réception → onglet Activité** : les événements `llm.backend_down` y apparaissent avec leur horodatage si vous avez activé la notification correspondante.
4. En dernier recours, contactez le support en joignant le message d'erreur visible au survol du statut + l'identifiant du backend.

> **Référence technique :** [Référence Apollia](../../reference/index.md) - comprendre comment Apollia isole les clés API et la mécanique de ping périodique.
