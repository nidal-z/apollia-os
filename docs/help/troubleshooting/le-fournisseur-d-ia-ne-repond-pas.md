# Le fournisseur d'IA ne répond pas

> Pour tout operator dont le chat reste figé ou affiche un bandeau rouge "Fournisseur déconnecté" : retrouver une IA fonctionnelle en moins de cinq minutes.

## Vérifications rapides (par ordre de probabilité)

### 1. Votre connexion internet est tombée

La plupart des fournisseurs (Anthropic, OpenAI) sont des services en ligne. Une coupure Wi-Fi ou VPN suffit à interrompre le chat.

**Solution :**
1. Ouvrez n'importe quel onglet de navigateur pour vérifier que vous êtes en ligne.
2. Si la connexion est revenue, attendez quelques secondes : le bandeau rouge disparaît tout seul et vous pouvez relancer votre message.

### 2. La clé API du fournisseur est invalide ou expirée

Une clé révoquée, expirée ou copiée avec un espace en trop fait échouer toutes les requêtes.

**Solution :**
1. Dans la sidebar, cliquez sur **Paramètres**, puis sur la section **Backends LLM**.
2. Repérez le backend marqué en rouge dans la liste, puis cliquez sur **Tester la connexion**.
   `[SCREENSHOT: page Paramètres Backends LLM, backend en erreur avec bouton Tester en évidence]`
3. Si le voyant reste rouge, ouvrez le backend et collez à nouveau une clé valide depuis la console du fournisseur.
4. Cliquez sur **Tester** une nouvelle fois : le voyant doit passer au vert.

### 3. Le nom du modèle est incorrect ou indisponible

Les fournisseurs renomment ou retirent régulièrement leurs modèles. Un identifiant obsolète provoque une erreur silencieuse.

**Solution :**
1. Ouvrez le backend en erreur depuis **Paramètres → Backends LLM**.
2. Vérifiez le champ **Modèle** : il doit correspondre exactement à un identifiant en cours (par exemple `claude-3-5-sonnet-latest` ou `gpt-4o`).
3. Corrigez la valeur, cliquez sur **Tester la connexion**, puis sur **Enregistrer**.

### 4. Le service du fournisseur est en panne

Anthropic, OpenAI et les autres fournisseurs publient des incidents sur leur page de statut. Si le test échoue alors que tout semble correct chez vous, l'origine est de leur côté.

**Solution :**
1. Consultez la page de statut du fournisseur concerné.
2. Si un incident est en cours, ajoutez un second backend de secours (un autre fournisseur ou un modèle local) dans **Paramètres → Backends LLM** pour ne pas rester bloqué.

### 5. Le service local (Ollama, modèle GGUF) n'est plus actif

Si vous utilisez un modèle local, le processus correspondant doit tourner sur votre machine.

**Solution :**
1. Pour Ollama, vérifiez que le service est démarré sur votre poste.
2. Pour un modèle local Apollia, ouvrez **Paramètres → Hub de modèles** et confirmez que le modèle est bien chargé.
3. Cliquez sur **Tester la connexion** sur le backend correspondant.

## Si rien ne fonctionne

1. Fermez complètement Apollia et relancez l'application : la connexion au fournisseur est rétablie au démarrage.
2. Si le bandeau rouge persiste, ouvrez **Observabilité → Audit trail** pour repérer le moment exact où la connexion a été perdue.
3. En dernier recours, contactez le support en joignant la dernière ligne d'erreur affichée dans le bandeau.

> **Concept :** [Securite-LLM-Backend](https://github.com/nidal-z/apollia-os/wiki/Securite-LLM-Backend) — comprendre comment Apollia isole les clés API et pourquoi un backend peut être marqué en erreur.
