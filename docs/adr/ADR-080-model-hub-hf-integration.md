# ADR-080 - Model Hub : intégration HuggingFace via token optionnel, zéro redistribution

**Date :** 2026-04-24
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** Sprint 43 - LLM Backend Management + Model Hub

---

## Contexte

Les utilisateurs d'Apollia doivent gérer manuellement leurs modèles GGUF : trouver le bon fichier
sur HuggingFace, choisir la bonne quantization selon leur machine, télécharger (parfois plusieurs
dizaines de GB), puis configurer les paramètres de génération à la main.

Deux approches ont été évaluées pour améliorer cette expérience :

1. **Registre statique embarqué** : une liste JSON de modèles recommandés maintenue à la main dans
   le dépôt, avec leurs paramètres de génération et tailles.
2. **Intégration HuggingFace directe** : lire les métadonnées depuis l'API HF publique au moment
   de la navigation, sans intermédiaire.

Par ailleurs, la question de la distribution des modèles se posait : héberger les modèles sur
notre infrastructure vs utiliser HF comme CDN.

---

## Décision

### 1. HuggingFace comme source de vérité dynamique

**Zéro registre statique.** Les métadonnées de modèles (liste de fichiers, tailles, paramètres de
génération, tags, licence) sont lues directement depuis l'API HF publique :

- `GET https://huggingface.co/api/models?filter=gguf&sort=downloads&search={q}` - recherche
- `GET https://huggingface.co/api/models/{repo_id}` - métadonnées complètes
- `GET https://huggingface.co/{repo_id}/resolve/main/generation_config.json` - params de génération

Les paramètres de génération (`temperature`, `top_k`, `top_p`, `repetition_penalty`, `max_new_tokens`)
sont lus directement depuis `generation_config.json` de chaque repo - toujours à jour, zéro maintenance.

Les métadonnées sont mises en cache dans `system.db` (table `model_metadata`) avec TTL 24h pour
éviter les re-fetch inutiles.

### 2. Token HF optionnel + wizard pour modèles gated

95 % des modèles populaires (Qwen3, Mistral, Phi-4, DeepSeek, Llama community variants) sont
Apache 2.0 ou MIT et ne nécessitent **aucun token**. L'API HF est publique pour ces modèles.

Pour les modèles **gated** (accès conditionné à une acceptation de licence sur huggingface.co,
comme Llama 3.1 Meta ou Mistral Large) :
- Le token HF est stocké dans `system.db` table `secrets` (déjà existante, chiffrée)
- Il est transmis via `Authorization: Bearer {token}` uniquement quand présent
- Un wizard 3 écrans s'affiche à la première tentative d'accès à un modèle gated :
  1. Explication : ce modèle nécessite une acceptation de licence sur HuggingFace
  2. Lien vers `huggingface.co/settings/tokens` pour créer un token
  3. Champ copier-coller du token

### 3. HF comme CDN direct - zéro redistribution

Les fichiers GGUF sont téléchargés **directement depuis HuggingFace** :
```
GET https://huggingface.co/{repo_id}/resolve/main/{filename}
```

Apollia ne redistribue aucun modèle. HuggingFace gère les licences côté utilisateur. Cette approche
est identique à ce que font LM Studio, Ollama, et Jan.

Le `DownloadManager` (`apollia-llm/src/downloader.rs`) :
- Streaming `reqwest` avec tracking de progression
- Reprise via `Range` header (si fichier partiel détecté)
- Annulation via `CancellationToken` tokio
- Events Tauri : `model-download-progress/{download_id}`
- Destination : `~/.apollia/models/` (configurable)

### 4. Hardware detection précise + badges de compatibilité

La détection hardware (`apollia-llm/src/hardware.rs`) identifie :
- **Apple Silicon** : modèle exact de puce (`M4 Max`, `M3 Pro`...) via `system_profiler SPHardwareDataType`
- **CUDA** : modèle GPU exact + VRAM + compute capability via `nvidia-smi`
- **CPU-only** : RAM totale via `sysinfo`

Budget mémoire :
- Apple Silicon : `total_ram_gb * 0.75` (mémoire unifiée)
- CUDA : VRAM du GPU principal
- CPU-only : `total_ram_gb * 0.60`

Badges de compatibilité par fichier GGUF (comme HuggingFace) :
- **Fits** : `file_size_gb * 1.1 < budget_gb * 0.70`
- **Might fit** : `file_size_gb * 1.1 < budget_gb * 1.00`
- **Too large** : sinon

### 5. Feature gate `cloud`

Toute la logique HF + downloader est sous `#[cfg(feature = "cloud")]`, activée par défaut.
Les utilisateurs sur réseau restreint peuvent compiler sans cette feature pour un binaire
purement local.

---

## Conséquences

### Positives
- **Zéro maintenance** : pas de registre à mettre à jour quand HF publie un nouveau modèle
- **Toujours à jour** : les params de génération reflètent la version actuelle du repo HF
- **Légalement sain** : HF gère les licences, Apollia ne redistribue rien
- **UX industrielle** : badges de compatibilité hardware-aware, auto-fill des params, progress bars
- **95 % sans token** : l'expérience principale ne nécessite pas de compte HF

### Négatives
- **Dépendance réseau** : la recherche et le téléchargement nécessitent une connexion. Atténué
  par le cache TTL 24h pour les métadonnées et la liste des modèles déjà téléchargés.
- **API HF non garantie** : HuggingFace peut modifier son API publique. Risque faible (HF a un
  intérêt commercial à la stabilité) et identique à tous les clients HF tiers.
- **Latence première ouverture** : le Model Hub charge les données depuis HF à la première visite.
  Skeleton screens + états de chargement explicites dans l'UI.

---

## Alternatives écartées

**Registre statique JSON** : maintenable à court terme, mais crée une dette dès qu'un nouveau
modèle populaire émerge (ex : DeepSeek R2, Qwen4). Rejeté au profit de l'API HF directe.

**Hébergement propre des modèles** : coût de stockage prohibitif (un modèle Qwen3-235B = ~130 GB),
enjeux légaux de redistribution per-licence, bande passante. Rejeté définitivement.

**Ollama comme couche d'abstraction** : Ollama gère déjà le téléchargement et le serving de modèles.
Rejeté car viole le Principe #2 (zéro dépendance externe) et crée un couplage fort à un binaire tiers.
