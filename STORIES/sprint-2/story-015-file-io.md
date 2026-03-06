# [Sprint 2][apollia-tools] file_io — lecture/ecriture avec protection path traversal

**ID :** STORY-015
**Sprint :** 2
**Crate cible :** `apollia-tools`
**Fichier(s) cible(s) :** `crates/apollia-tools/src/tools/file_io.rs`
**Taille :** M
**Depend de :** STORY-010
**Statut :** 🔲 A faire

---

## User Story

```
En tant que runtime,
je veux un outil file_io qui permet a un agent de lire et ecrire des fichiers
uniquement dans son repertoire sandbox dedie,
afin qu'aucun agent ne puisse acceder aux fichiers d'un autre agent
ou aux fichiers systeme de l'hote.
```

---

## Contexte technique

Outil natif d'IO. La securite est garantie par la validation de chemin au niveau Rust
(resolution canonique + verification de prefixe) — sans dependance a des mecanismes OS.
Chaque agent dispose de son sandbox sous `~/.apollia/sandboxes/<agent_id>/workspace/`.

**Principe(s) architecturaux concernes :**
- Principe #1 — Local-first (fichiers locaux uniquement)
- Principe #4 — Fail fast (path traversal detecte avant tout IO)

**Position dans l'architecture :**
```
apollia-tools
  └── tools/
        └── file_io.rs  <- cette story
              ├── FileIo         (struct publique)
              ├── FileIoError    (erreurs)
              └── resolve_path() (validation anti-traversal)
```

---

## Criteres d'Acceptation

### AC-1 — Lecture d'un fichier existant dans le sandbox

```
ETANT DONNE un FileIo configure avec sandbox_root = /tmp/test_agent/workspace/
ET un fichier "config.json" present dans ce repertoire
QUAND on appelle file_io.read("config.json")
ALORS le contenu du fichier est retourne comme Vec<u8>
```

### AC-2 — Path traversal rejete

```
ETANT DONNE un FileIo configure avec sandbox_root = /tmp/test_agent/workspace/
QUAND on appelle file_io.read("../../etc/passwd")
ALORS Err(FileIoError::SandboxViolation { path: "../../etc/passwd" }) est retourne
ET aucun IO n'est effectue
```

### AC-3 — Ecriture dans le sandbox

```
ETANT DONNE un FileIo configure
QUAND on appelle file_io.write("output/result.json", b"{}")
ALORS le fichier est cree (avec creation des sous-repertoires si necessaire)
ET file_io.read("output/result.json") retourne b"{}"
```

### AC-4 — Lecture de fichier inexistant retourne erreur claire

```
ETANT DONNE un FileIo configure
QUAND on appelle file_io.read("nonexistent.txt")
ALORS Err(FileIoError::NotFound { path: "nonexistent.txt" }) est retourne
```

### AC-5 — Listage des fichiers dans le sandbox

```
ETANT DONNE un sandbox avec 3 fichiers .json
QUAND on appelle file_io.list(".", "*.json")
ALORS une Vec<String> avec les 3 noms de fichiers est retournee
```

### AC-6 — Chemin absolu hors sandbox rejete

```
ETANT DONNE un FileIo configure avec sandbox_root = /tmp/test_agent/workspace/
QUAND on appelle file_io.read("/etc/passwd")
ALORS Err(FileIoError::SandboxViolation { ... }) est retourne
```

---

## Specification technique

### Types a creer dans `crates/apollia-tools/src/tools/file_io.rs`

```rust
/// Outil natif d'IO fichier avec isolation par repertoire sandbox.
///
/// Toute tentative d'acces hors du sandbox_root retourne SandboxViolation.
/// La protection est implementee par resolution canonique du chemin (pas d'acces OS).
pub struct FileIo {
    /// Repertoire racine du sandbox de l'agent.
    /// Tous les chemins sont resolus relativement a ce repertoire.
    sandbox_root: PathBuf,
}

/// Erreurs de l'outil file_io.
pub enum FileIoError {
    /// Tentative d'acces hors du sandbox de l'agent.
    SandboxViolation { path: String },
    /// Fichier ou repertoire introuvable.
    NotFound { path: String },
    /// Erreur IO generique (permission, disque plein, etc.)
    IoError { path: String, cause: String },
}

impl FileIo {
    /// Cree un FileIo avec le sandbox_root de l'agent.
    ///
    /// Cree le repertoire sandbox_root s'il n'existe pas.
    pub fn new(sandbox_root: PathBuf) -> Result<Self, FileIoError> { ... }

    /// Lit le contenu d'un fichier dans le sandbox.
    pub async fn read(&self, path: &str) -> Result<Vec<u8>, FileIoError> { ... }

    /// Ecrit des donnees dans un fichier du sandbox.
    ///
    /// Cree les sous-repertoires intermediaires si necessaire.
    pub async fn write(&self, path: &str, content: &[u8]) -> Result<(), FileIoError> { ... }

    /// Liste les fichiers correspondant a un pattern glob dans le sandbox.
    pub async fn list(&self, dir: &str, pattern: &str) -> Result<Vec<String>, FileIoError> { ... }

    /// Retourne le ToolDescriptor de cet outil.
    pub fn descriptor() -> ToolDescriptor { ... }

    /// Valide et resout un chemin relatif au sandbox_root.
    ///
    /// Retourne le chemin absolu canonique si valide, SandboxViolation sinon.
    fn resolve_path(&self, path: &str) -> Result<PathBuf, FileIoError> { ... }
}
```

### Dependances Cargo

```toml
# Aucune nouvelle dependance — tokio (fs feature incluse dans "full") est deja declare
```

### Comportement attendu — Protection path traversal

```rust
fn resolve_path(&self, path: &str) -> Result<PathBuf, FileIoError> {
    // 1. Rejeter les chemins absolus directement
    if Path::new(path).is_absolute() {
        return Err(FileIoError::SandboxViolation { path: path.to_string() });
    }

    // 2. Construire le chemin cible
    let target = self.sandbox_root.join(path);

    // 3. Normaliser sans suivre les symlinks (pour detecter "..")
    // Utiliser std::path::Path::components() pour filtrer ".." et composants absolus
    // OU canonicaliser apres creation (si le fichier existe)

    // 4. Verifier que le chemin normalise commence par sandbox_root
    if !target_normalized.starts_with(&self.sandbox_root) {
        return Err(FileIoError::SandboxViolation { path: path.to_string() });
    }

    Ok(target_normalized)
}
```

Note implementation : Pour les fichiers non encore crees (write), la canonicalisation
doit se faire sur le chemin normalise sans IO (pas de `canonicalize()` qui necessite
que le fichier existe). Utiliser `path::components()` pour eliminer les `..`.

### Ce que cette story N'implemente PAS

- Export vers espace shared (STORY future)
- Encodage specifique (retourne toujours bytes, l'agent gere l'encodage)
- Audit trail (STORY-016)
- Limites de taille de fichier (MVP sans quota)

---

## Tests requis

### Tests unitaires dans `crates/apollia-tools/src/tools/file_io.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tokio::fs;

    fn test_sandbox() -> PathBuf {
        std::env::temp_dir().join(format!("apollia_file_io_test_{}", uuid::Uuid::new_v4()))
    }

    #[tokio::test]
    async fn test_ac1_read_existing_file() {
        // GIVEN
        let sandbox = test_sandbox();
        let file_io = FileIo::new(sandbox.clone()).expect("FileIo creation failed");
        fs::write(sandbox.join("config.json"), b"{}").await.unwrap();
        // WHEN
        let content = file_io.read("config.json").await.expect("read failed");
        // THEN
        assert_eq!(content, b"{}");
        fs::remove_dir_all(&sandbox).await.ok();
    }

    #[tokio::test]
    async fn test_ac2_path_traversal_rejected() {
        // GIVEN
        let sandbox = test_sandbox();
        let file_io = FileIo::new(sandbox.clone()).expect("FileIo creation failed");
        // WHEN
        let result = file_io.read("../../etc/passwd").await;
        // THEN
        assert!(matches!(result, Err(FileIoError::SandboxViolation { .. })));
        fs::remove_dir_all(&sandbox).await.ok();
    }

    #[tokio::test]
    async fn test_ac3_write_creates_file_and_dirs() {
        // GIVEN
        let sandbox = test_sandbox();
        let file_io = FileIo::new(sandbox.clone()).expect("FileIo creation failed");
        // WHEN
        file_io.write("output/result.json", b"{}").await.expect("write failed");
        // THEN
        let content = file_io.read("output/result.json").await.expect("read after write failed");
        assert_eq!(content, b"{}");
        fs::remove_dir_all(&sandbox).await.ok();
    }

    #[tokio::test]
    async fn test_ac4_read_nonexistent_file_returns_not_found() {
        // GIVEN
        let sandbox = test_sandbox();
        let file_io = FileIo::new(sandbox.clone()).expect("FileIo creation failed");
        // WHEN
        let result = file_io.read("nonexistent.txt").await;
        // THEN
        assert!(matches!(result, Err(FileIoError::NotFound { .. })));
        fs::remove_dir_all(&sandbox).await.ok();
    }

    #[tokio::test]
    async fn test_ac6_absolute_path_rejected() {
        // GIVEN
        let sandbox = test_sandbox();
        let file_io = FileIo::new(sandbox.clone()).expect("FileIo creation failed");
        // WHEN
        let result = file_io.read("/etc/passwd").await;
        // THEN
        assert!(matches!(result, Err(FileIoError::SandboxViolation { .. })));
        fs::remove_dir_all(&sandbox).await.ok();
    }

    #[test]
    fn test_descriptor_is_valid() {
        let descriptor = FileIo::descriptor();
        assert_eq!(descriptor.name, "file_io");
        assert!(descriptor.validate().is_ok());
    }
}
```

---

## Definition of Done

**Qualite code :**
- [ ] `cargo test -p apollia-tools` passe (0 test ignore)
- [ ] `cargo clippy -p apollia-tools -- -D warnings` : zero warning
- [ ] `cargo fmt --check` : code formate
- [ ] Zero `unwrap()` dans le code de production
- [ ] Zero `todo!()` dans le code de production
- [ ] Docstring `///` sur chaque struct, enum, et fonction publique

**Architectural :**
- [ ] Path traversal detecte avant tout IO (Principe #4)
- [ ] Chemins absolus rejetes en premiere ligne de `resolve_path()`
- [ ] Tests creent leurs propres sandboxes temporaires et les nettoient

**Commit :**
- [ ] Commit conventionnel : `feat(apollia-tools): add file_io with path traversal protection`

---

## Notes d'implementation

**Decisions prises pendant l'implementation :**

**Deviations par rapport a la spec :**

**Dette technique identifiee :**

---

## Liens

- Epic parent : Sprint 2 — Tool Registry + Outils natifs
- Story precedente : STORY-011 (ToolRegistry)
- Story suivante : STORY-013 (bash_executor)
- ADR associe : aucun prevu
