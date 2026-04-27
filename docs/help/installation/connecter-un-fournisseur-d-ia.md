# Connecter un fournisseur d'IA

> Pour tout operator qui vient d'installer Apollia : brancher un premier fournisseur d'IA (Anthropic, OpenAI, Ollama, modèle local) afin que le chat et les agents puissent répondre.

## Prérequis

- Apollia est lancé et le bandeau supérieur affiche le statut connexion.
- Vous disposez de la clé API du fournisseur souhaité (ou rien si vous utilisez un modèle local).
- Connexion internet active si vous utilisez un fournisseur cloud.

## Étapes

1. Dans la sidebar, cliquez sur **Settings**, puis sur l'onglet **Backends LLM**.
   `[SCREENSHOT: page Settings, onglet Backends LLM actif, liste vide ou avec backends existants]`

2. Cliquez sur le bouton **Ajouter un backend** en haut à droite. Une fenêtre de configuration s'ouvre.

3. Donnez un **nom** clair à ce backend (par exemple : *Claude Anthropic* ou *Ollama local*). Ce nom apparaîtra dans le sélecteur de chat.

4. Choisissez le **fournisseur** dans la liste : Anthropic, OpenAI, Ollama, ou modèle local.
   `[SCREENSHOT: dialog Ajouter un backend, champs Nom et Fournisseur remplis, sélecteur déroulé]`

5. Collez la **clé API** du fournisseur. Pour un modèle local ou Ollama installé sur votre machine, laissez le champ vide.

6. Saisissez le **nom du modèle** à utiliser. Exemples :
   - **Anthropic :** `claude-3-5-sonnet-20241022` ou `claude-opus-4-7`
   - **OpenAI :** `gpt-4o-mini` (affiché comme placeholder par défaut)
   - **Ollama :** `llama3.1:8b` ou tout modèle installé localement
   - **Modèle local (llama.cpp) :** chemin absolu vers le fichier `.gguf`

7. Cliquez sur **Tester la connexion**. Un voyant vert confirme que le fournisseur répond ; un voyant rouge signale un problème.
   `[SCREENSHOT: dialog avec voyant vert "Connexion réussie" sous le bouton Tester]`

8. Si le test passe, cliquez sur **Créer**. Le backend apparaît dans la liste.

9. (Optionnel) Cochez **Backend par défaut** pour qu'il soit sélectionné automatiquement à l'ouverture d'un nouveau chat.

## Vérification

Le bandeau supérieur affiche désormais une pastille verte avec le nom de votre backend. Ouvrez un chat et envoyez un message court : la réponse arrive en streaming.

## Si ça ne marche pas

- **Voyant rouge au test :** vérifiez la clé API (espaces parasites, clé révoquée) et le nom exact du modèle.
- **Pas de réponse dans le chat malgré pastille verte :** consultez [Le fournisseur d'IA ne répond pas](../troubleshooting/le-fournisseur-d-ia-ne-repond-pas.md).
- **Ollama local introuvable :** assurez-vous que le service Ollama tourne (`ollama serve`) avant de tester.

> **Référence technique :** [Briques-LLM-Backend](https://github.com/nidal-z/apollia-os/wiki/Briques-LLM-Backend) — table complète des fournisseurs supportés, modèles, paramètres avancés (routing, fallback).
