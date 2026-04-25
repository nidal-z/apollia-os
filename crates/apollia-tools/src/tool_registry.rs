//! Runtime gouvernance des outils natifs : activation/désactivation et secrets.
//!
//! Ce module expose deux composants persistés dans `governance.db` :
//!
//! - [`ToolRegistry`] — état `enabled`/`disabled` et configuration JSON par outil.
//!   Règle d'activation : absent de la table `tools` OU `enabled = TRUE` → actif ;
//!   seul `enabled = FALSE` désactive. La table sert uniquement de liste
//!   d'exception ; les outils inconnus restent actifs par défaut.
//! - [`ToolCredentialStore`] — secrets par outil (par exemple
//!   `web_search/brave.api_key`), chiffrés AES-256-GCM avec une clé maître de
//!   32 octets stockée dans un fichier `~/.apollia/.keyfile` (chmod 600). Le
//!   nonce de 12 octets est généré aléatoirement par insertion et préfixé au
//!   ciphertext en base.
//!
//! Les deux composants partagent la base mais possèdent leur propre connexion
//! SQLite : ils sont indépendants et peuvent vivre dans des acteurs distincts.

use std::path::{Path, PathBuf};

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use rand::RngCore;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};

/// Liste des outils natifs connus du runtime, utilisée par
/// [`ToolRegistry::list`] pour produire un statut même quand aucune entrée
/// n'existe en base.
///
/// Toute modification de [`crate::native_dispatcher::build_native_dispatcher`]
/// doit être répercutée ici pour rester cohérente.
pub const NATIVE_TOOL_NAMES: &[&str] = &[
    "bash_executor",
    "python_executor",
    "file_read",
    "file_write",
    "file_list",
    "file_edit",
    "file_glob",
    "file_grep",
    "notebook_read",
    "notebook_edit",
    "http_fetch",
    "web_search",
    "web_read",
    "memory_search",
    "ask_user",
];

/// Erreur retournée par [`ToolRegistry`] et [`ToolCredentialStore`].
#[derive(Debug, thiserror::Error)]
pub enum ToolGovernanceError {
    /// Erreur SQLite au cours d'une requête de gouvernance.
    #[error("governance database error: {0}")]
    Database(#[from] rusqlite::Error),
    /// Erreur d'I/O lors de la lecture/écriture du fichier `.keyfile`.
    #[error("keyfile I/O error at {path}: {source}")]
    Keyfile {
        /// Chemin du `.keyfile`.
        path: PathBuf,
        /// Cause sous-jacente.
        #[source]
        source: std::io::Error,
    },
    /// La clé maître lue depuis `.keyfile` n'a pas la taille attendue.
    #[error("keyfile is corrupted: expected 32 bytes, found {found}")]
    KeyfileCorrupted {
        /// Taille observée.
        found: usize,
    },
    /// La valeur stockée est trop courte pour contenir un nonce + ciphertext.
    #[error("encrypted value is corrupted (too short)")]
    CiphertextCorrupted,
    /// Sérialisation JSON de la configuration outil impossible.
    #[error("invalid tool config JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
    /// Le déchiffrement AES-256-GCM a échoué (clé incorrecte ou ciphertext altéré).
    #[error("decryption failed (wrong key or tampered ciphertext)")]
    DecryptFailed,
    /// L'AES-256-GCM n'a pas pu produire le ciphertext.
    #[error("encryption failed")]
    EncryptFailed,
}

/// Snapshot de l'état d'un outil tel que présenté par [`ToolRegistry::list`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolStatus {
    /// Nom canonique de l'outil (ex. `bash_executor`).
    pub name: String,
    /// `true` si l'outil est actif. Voir la règle d'activation du module.
    pub enabled: bool,
    /// Configuration JSON spécifique à l'outil, `None` si non définie.
    pub config: Option<serde_json::Value>,
    /// Timestamp Unix (secondes) de la dernière modification de la ligne
    /// `tools` correspondante. Vaut `0` quand l'outil n'a pas d'entrée.
    pub updated_at: i64,
}

/// Une entrée de [`ToolCredentialStore::list`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialEntry {
    /// Nom de l'outil propriétaire de la credential.
    pub tool_name: String,
    /// Nom logique de la clé (ex. `brave.api_key`).
    pub key_name: String,
    /// Timestamp Unix (secondes) de création.
    pub created_at: i64,
    /// Timestamp Unix (secondes) de la dernière utilisation effective, le cas
    /// échéant.
    pub last_used_at: Option<i64>,
}

/// Registre persisté des outils activés/désactivés et de leur config JSON.
pub struct ToolRegistry {
    conn: Connection,
}

impl ToolRegistry {
    /// Ouvre la base `governance.db` à *db_path* en lecture/écriture et
    /// retourne le registre.
    ///
    /// La table `tools` doit déjà exister (voir
    /// [`crate::governance_db::GovernanceDb`]).
    ///
    /// # Errors
    ///
    /// Retourne [`ToolGovernanceError::Database`] si SQLite échoue à ouvrir
    /// la base.
    pub fn new(db_path: &Path) -> Result<Self, ToolGovernanceError> {
        let conn = Connection::open_with_flags(
            db_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        Ok(Self { conn })
    }

    /// Indique si l'outil *tool_name* est actif.
    ///
    /// Un outil absent de la table `tools` est considéré actif par défaut ; un
    /// outil dont `enabled = FALSE` est inactif.
    ///
    /// # Errors
    ///
    /// Retourne [`ToolGovernanceError::Database`] si la requête échoue.
    pub fn is_enabled(&self, tool_name: &str) -> Result<bool, ToolGovernanceError> {
        let row = self
            .conn
            .query_row(
                "SELECT enabled FROM tools WHERE name = ?1",
                params![tool_name],
                |r| r.get::<_, bool>(0),
            )
            .optional()?;
        Ok(row.unwrap_or(true))
    }

    /// Active ou désactive *tool_name*.
    ///
    /// L'écriture est un upsert atomique : la ligne existante est mise à jour
    /// si présente, sinon insérée. `updated_at` est mis à `unixepoch()`.
    ///
    /// # Errors
    ///
    /// Retourne [`ToolGovernanceError::Database`] si la requête échoue.
    pub fn set_enabled(
        &mut self,
        tool_name: &str,
        enabled: bool,
    ) -> Result<(), ToolGovernanceError> {
        self.conn.execute(
            "INSERT INTO tools (name, enabled, config_json, updated_at)
             VALUES (?1, ?2, NULL, unixepoch())
             ON CONFLICT(name) DO UPDATE SET enabled = excluded.enabled, updated_at = unixepoch()",
            params![tool_name, enabled],
        )?;
        Ok(())
    }

    /// Retourne la configuration JSON stockée pour *tool_name*, ou `None`.
    ///
    /// # Errors
    ///
    /// Retourne [`ToolGovernanceError::Database`] si la lecture échoue ou
    /// [`ToolGovernanceError::InvalidJson`] si le JSON stocké est mal formé.
    pub fn get_config(
        &self,
        tool_name: &str,
    ) -> Result<Option<serde_json::Value>, ToolGovernanceError> {
        let row: Option<Option<String>> = self
            .conn
            .query_row(
                "SELECT config_json FROM tools WHERE name = ?1",
                params![tool_name],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()?;
        match row.flatten() {
            None => Ok(None),
            Some(raw) => Ok(Some(serde_json::from_str(&raw)?)),
        }
    }

    /// Stocke la configuration JSON associée à *tool_name*.
    ///
    /// # Errors
    ///
    /// Retourne [`ToolGovernanceError::Database`] si l'écriture échoue ou
    /// [`ToolGovernanceError::InvalidJson`] si la valeur ne peut être
    /// sérialisée.
    pub fn set_config(
        &mut self,
        tool_name: &str,
        cfg: &serde_json::Value,
    ) -> Result<(), ToolGovernanceError> {
        let raw = serde_json::to_string(cfg)?;
        self.conn.execute(
            "INSERT INTO tools (name, enabled, config_json, updated_at)
             VALUES (?1, TRUE, ?2, unixepoch())
             ON CONFLICT(name) DO UPDATE SET config_json = excluded.config_json, updated_at = unixepoch()",
            params![tool_name, raw],
        )?;
        Ok(())
    }

    /// Liste l'union des outils enregistrés et des outils natifs connus.
    ///
    /// Pour un outil sans entrée en base, le statut renvoyé a `enabled = true`,
    /// `config = None` et `updated_at = 0`.
    ///
    /// # Errors
    ///
    /// Retourne [`ToolGovernanceError::Database`] si la lecture échoue ou
    /// [`ToolGovernanceError::InvalidJson`] si une config stockée est mal
    /// formée.
    pub fn list(&self) -> Result<Vec<ToolStatus>, ToolGovernanceError> {
        let mut stmt = self
            .conn
            .prepare("SELECT name, enabled, config_json, updated_at FROM tools ORDER BY name")?;
        let rows = stmt.query_map([], |row| {
            let name: String = row.get(0)?;
            let enabled: bool = row.get(1)?;
            let raw: Option<String> = row.get(2)?;
            let updated_at: i64 = row.get(3)?;
            Ok((name, enabled, raw, updated_at))
        })?;

        let mut by_name: std::collections::BTreeMap<String, ToolStatus> =
            std::collections::BTreeMap::new();
        for row in rows {
            let (name, enabled, raw, updated_at) = row?;
            let config = match raw {
                Some(s) => Some(serde_json::from_str(&s)?),
                None => None,
            };
            by_name.insert(
                name.clone(),
                ToolStatus {
                    name,
                    enabled,
                    config,
                    updated_at,
                },
            );
        }

        for native in NATIVE_TOOL_NAMES {
            by_name.entry((*native).to_string()).or_insert(ToolStatus {
                name: (*native).to_string(),
                enabled: true,
                config: None,
                updated_at: 0,
            });
        }

        Ok(by_name.into_values().collect())
    }
}

/// Magasin chiffré de credentials par outil (AES-256-GCM).
///
/// Chaque valeur stockée en base est `nonce(12) || ciphertext` ; le tag
/// d'authentification GCM est inclus dans le ciphertext par la crate
/// `aes-gcm`. La clé maître est lue depuis un fichier dédié, créé avec
/// `chmod 600` au premier appel s'il n'existe pas.
pub struct ToolCredentialStore {
    conn: Connection,
    cipher: Aes256Gcm,
}

impl ToolCredentialStore {
    /// Ouvre le store en lecture/écriture sur *db_path* en utilisant la clé
    /// maître stockée dans *keyfile_path*.
    ///
    /// Le `.keyfile` est créé (mode `0o600`) avec une clé aléatoire de 32
    /// octets s'il n'existe pas. S'il existe, son contenu doit faire
    /// exactement 32 octets.
    ///
    /// # Errors
    ///
    /// - [`ToolGovernanceError::Database`] si SQLite échoue.
    /// - [`ToolGovernanceError::Keyfile`] si la lecture/écriture du fichier
    ///   échoue.
    /// - [`ToolGovernanceError::KeyfileCorrupted`] si la clé n'a pas la
    ///   taille attendue.
    pub fn new(db_path: &Path, keyfile_path: &Path) -> Result<Self, ToolGovernanceError> {
        let conn = Connection::open_with_flags(
            db_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?;
        let key_bytes = load_or_create_keyfile(keyfile_path)?;
        let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
        let cipher = Aes256Gcm::new(key);
        Ok(Self { conn, cipher })
    }

    /// Stocke (insertion ou remplacement) la credential `(tool, key)`.
    ///
    /// Un nouveau nonce de 12 octets est généré à chaque appel.
    ///
    /// # Errors
    ///
    /// - [`ToolGovernanceError::EncryptFailed`] si AES-256-GCM échoue
    ///   (cas pratiquement impossible avec la crate `aes-gcm`).
    /// - [`ToolGovernanceError::Database`] si l'écriture SQLite échoue.
    pub fn set(&mut self, tool: &str, key: &str, value: &str) -> Result<(), ToolGovernanceError> {
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = self
            .cipher
            .encrypt(nonce, value.as_bytes())
            .map_err(|_| ToolGovernanceError::EncryptFailed)?;

        let mut blob = Vec::with_capacity(12 + ciphertext.len());
        blob.extend_from_slice(&nonce_bytes);
        blob.extend_from_slice(&ciphertext);

        self.conn.execute(
            "INSERT INTO tool_credentials (tool_name, key_name, value_encrypted, created_at)
             VALUES (?1, ?2, ?3, unixepoch())
             ON CONFLICT(tool_name, key_name) DO UPDATE SET
               value_encrypted = excluded.value_encrypted,
               created_at      = unixepoch(),
               last_used_at    = NULL",
            params![tool, key, blob],
        )?;
        Ok(())
    }

    /// Récupère la valeur claire associée à `(tool, key)`, ou `None` si la
    /// credential n'existe pas.
    ///
    /// # Errors
    ///
    /// - [`ToolGovernanceError::CiphertextCorrupted`] si la valeur en base est
    ///   trop courte.
    /// - [`ToolGovernanceError::DecryptFailed`] si AES-256-GCM rejette le
    ///   tag d'authentification.
    /// - [`ToolGovernanceError::Database`] si la lecture SQLite échoue.
    pub fn get(&self, tool: &str, key: &str) -> Result<Option<String>, ToolGovernanceError> {
        let row: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT value_encrypted FROM tool_credentials WHERE tool_name = ?1 AND key_name = ?2",
                params![tool, key],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .optional()?;
        let blob = match row {
            Some(b) => b,
            None => return Ok(None),
        };
        if blob.len() < 12 {
            return Err(ToolGovernanceError::CiphertextCorrupted);
        }
        let (nonce_bytes, ciphertext) = blob.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);
        let plaintext = self
            .cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| ToolGovernanceError::DecryptFailed)?;
        let s = String::from_utf8(plaintext).map_err(|_| ToolGovernanceError::DecryptFailed)?;
        Ok(Some(s))
    }

    /// Supprime la credential `(tool, key)`. Retourne `true` si une ligne a
    /// été effacée.
    ///
    /// # Errors
    ///
    /// Retourne [`ToolGovernanceError::Database`] si SQLite échoue.
    pub fn delete(&mut self, tool: &str, key: &str) -> Result<bool, ToolGovernanceError> {
        let n = self.conn.execute(
            "DELETE FROM tool_credentials WHERE tool_name = ?1 AND key_name = ?2",
            params![tool, key],
        )?;
        Ok(n > 0)
    }

    /// Liste les credentials, filtrées par outil si *tool* est `Some`.
    ///
    /// Les valeurs chiffrées ne sont jamais retournées : seules les
    /// métadonnées le sont, ce qui permet d'afficher un état "credential
    /// présente" sans exposer le secret.
    ///
    /// # Errors
    ///
    /// Retourne [`ToolGovernanceError::Database`] si la requête échoue.
    pub fn list(&self, tool: Option<&str>) -> Result<Vec<CredentialEntry>, ToolGovernanceError> {
        let mut entries = Vec::new();
        match tool {
            Some(t) => {
                let mut stmt = self.conn.prepare(
                    "SELECT tool_name, key_name, created_at, last_used_at \
                     FROM tool_credentials WHERE tool_name = ?1 ORDER BY key_name",
                )?;
                let rows = stmt.query_map(params![t], map_credential_row)?;
                for row in rows {
                    entries.push(row?);
                }
            }
            None => {
                let mut stmt = self.conn.prepare(
                    "SELECT tool_name, key_name, created_at, last_used_at \
                     FROM tool_credentials ORDER BY tool_name, key_name",
                )?;
                let rows = stmt.query_map([], map_credential_row)?;
                for row in rows {
                    entries.push(row?);
                }
            }
        }
        Ok(entries)
    }

    /// Met à jour `last_used_at` pour la credential `(tool, key)`.
    ///
    /// Sans effet si la credential n'existe pas.
    ///
    /// # Errors
    ///
    /// Retourne [`ToolGovernanceError::Database`] si l'écriture échoue.
    pub fn touch_last_used(&mut self, tool: &str, key: &str) -> Result<(), ToolGovernanceError> {
        self.conn.execute(
            "UPDATE tool_credentials SET last_used_at = unixepoch() \
             WHERE tool_name = ?1 AND key_name = ?2",
            params![tool, key],
        )?;
        Ok(())
    }
}

fn map_credential_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CredentialEntry> {
    Ok(CredentialEntry {
        tool_name: row.get(0)?,
        key_name: row.get(1)?,
        created_at: row.get(2)?,
        last_used_at: row.get(3)?,
    })
}

fn load_or_create_keyfile(path: &Path) -> Result<[u8; 32], ToolGovernanceError> {
    if path.exists() {
        let bytes = std::fs::read(path).map_err(|e| ToolGovernanceError::Keyfile {
            path: path.to_path_buf(),
            source: e,
        })?;
        if bytes.len() != 32 {
            return Err(ToolGovernanceError::KeyfileCorrupted { found: bytes.len() });
        }
        let mut out = [0u8; 32];
        out.copy_from_slice(&bytes);
        return Ok(out);
    }

    if let Some(parent) = path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent).map_err(|e| ToolGovernanceError::Keyfile {
                path: parent.to_path_buf(),
                source: e,
            })?;
        }
    }

    let mut key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut key);
    write_keyfile_secure(path, &key)?;
    Ok(key)
}

#[cfg(unix)]
fn write_keyfile_secure(path: &Path, key: &[u8; 32]) -> Result<(), ToolGovernanceError> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| ToolGovernanceError::Keyfile {
            path: path.to_path_buf(),
            source: e,
        })?;
    use std::io::Write;
    file.write_all(key)
        .map_err(|e| ToolGovernanceError::Keyfile {
            path: path.to_path_buf(),
            source: e,
        })?;
    file.sync_all().map_err(|e| ToolGovernanceError::Keyfile {
        path: path.to_path_buf(),
        source: e,
    })?;
    Ok(())
}

#[cfg(not(unix))]
fn write_keyfile_secure(path: &Path, key: &[u8; 32]) -> Result<(), ToolGovernanceError> {
    std::fs::write(path, key).map_err(|e| ToolGovernanceError::Keyfile {
        path: path.to_path_buf(),
        source: e,
    })
}

/// Snapshot lu depuis `governance.db` à utiliser pour configurer un
/// [`crate::native_dispatcher::build_native_dispatcher`].
#[derive(Debug, Clone, Default)]
pub struct GovernanceSnapshot {
    /// Liste des outils dont `enabled = FALSE` au moment du snapshot.
    pub disabled_tools: Vec<String>,
    /// Clé d'API Brave Search déchiffrée, si présente dans `tool_credentials`.
    pub brave_api_key: Option<String>,
}

/// Lit `governance.db` et `.keyfile` dans *base_dir* pour produire un
/// [`GovernanceSnapshot`] consommable par le dispatcher.
///
/// Si la base ou le `.keyfile` n'existent pas, un snapshot vide est retourné
/// (tous les outils restent actifs, aucune clé Brave). Cette tolérance permet
/// au runtime de fonctionner avant la première écriture.
///
/// # Errors
///
/// Remonte les erreurs SQLite ou cryptographiques rencontrées lors de la
/// lecture quand la base existe mais n'est pas exploitable.
pub fn load_governance_snapshot(
    base_dir: &Path,
) -> Result<GovernanceSnapshot, ToolGovernanceError> {
    let db_path = base_dir.join(crate::governance_db::GOVERNANCE_DB_FILENAME);
    if !db_path.exists() {
        return Ok(GovernanceSnapshot::default());
    }

    let registry = ToolRegistry::new(&db_path)?;
    let disabled_tools = registry
        .list()?
        .into_iter()
        .filter(|s| !s.enabled)
        .map(|s| s.name)
        .collect();

    let keyfile = base_dir.join(".keyfile");
    let store = ToolCredentialStore::new(&db_path, &keyfile)?;
    let brave_api_key = store.get("web_search", "brave.api_key")?;

    Ok(GovernanceSnapshot {
        disabled_tools,
        brave_api_key,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governance_db::GovernanceDb;
    use rusqlite::params;
    use tempfile::TempDir;

    fn fresh(dir: &TempDir) -> (PathBuf, PathBuf) {
        let _ = GovernanceDb::open(dir.path()).expect("init governance.db");
        (
            dir.path()
                .join(crate::governance_db::GOVERNANCE_DB_FILENAME),
            dir.path().join(".keyfile"),
        )
    }

    #[test]
    fn test_tool_enabled_by_default_if_absent() {
        // GIVEN une base sans entrée pour `web_search`.
        let dir = TempDir::new().expect("tempdir");
        let (db, _) = fresh(&dir);
        let reg = ToolRegistry::new(&db).expect("open registry");
        // WHEN on demande l'état.
        let enabled = reg.is_enabled("web_search").expect("query");
        // THEN l'outil est considéré actif.
        assert!(enabled);
    }

    #[test]
    fn test_set_enabled_disables_tool() {
        // GIVEN un registre vierge.
        let dir = TempDir::new().expect("tempdir");
        let (db, _) = fresh(&dir);
        let mut reg = ToolRegistry::new(&db).expect("open");
        // WHEN on désactive bash_executor.
        reg.set_enabled("bash_executor", false).expect("disable");
        // THEN is_enabled retourne false ; un nouvel outil reste actif.
        assert!(!reg.is_enabled("bash_executor").expect("read"));
        assert!(reg.is_enabled("file_read").expect("read other"));
    }

    #[test]
    fn test_list_unions_native_and_db() {
        // GIVEN un registre où seul bash_executor est désactivé.
        let dir = TempDir::new().expect("tempdir");
        let (db, _) = fresh(&dir);
        let mut reg = ToolRegistry::new(&db).expect("open");
        reg.set_enabled("bash_executor", false).expect("disable");
        // WHEN on liste.
        let entries = reg.list().expect("list");
        // THEN tous les outils natifs apparaissent et bash_executor est inactif.
        let bash = entries
            .iter()
            .find(|e| e.name == "bash_executor")
            .expect("bash present");
        assert!(!bash.enabled);
        for native in NATIVE_TOOL_NAMES {
            assert!(
                entries.iter().any(|e| e.name == *native),
                "native tool {native} must be listed"
            );
        }
    }

    #[test]
    fn test_set_get_config_roundtrip() {
        // GIVEN un registre vierge.
        let dir = TempDir::new().expect("tempdir");
        let (db, _) = fresh(&dir);
        let mut reg = ToolRegistry::new(&db).expect("open");
        // WHEN on stocke puis lit la config web_search.
        let cfg = serde_json::json!({"default_backend": "duckduckgo"});
        reg.set_config("web_search", &cfg).expect("set");
        let read = reg.get_config("web_search").expect("get");
        // THEN la valeur lue est identique.
        assert_eq!(read, Some(cfg));
    }

    #[test]
    fn test_credential_roundtrip_encrypt_decrypt() {
        // GIVEN un store fraîchement créé.
        let dir = TempDir::new().expect("tempdir");
        let (db, kf) = fresh(&dir);
        let mut store = ToolCredentialStore::new(&db, &kf).expect("open store");
        // WHEN on stocke puis relit la valeur.
        store
            .set("web_search", "brave.api_key", "BSA-secret-1234")
            .expect("set");
        let read = store.get("web_search", "brave.api_key").expect("get");
        // THEN la valeur claire est identique.
        assert_eq!(read.as_deref(), Some("BSA-secret-1234"));
    }

    #[test]
    fn test_credential_not_in_plaintext_in_db() {
        // GIVEN une credential stockée.
        let dir = TempDir::new().expect("tempdir");
        let (db, kf) = fresh(&dir);
        {
            let mut store = ToolCredentialStore::new(&db, &kf).expect("open");
            store
                .set("web_search", "brave.api_key", "PLAINTEXT-MARKER-XYZ")
                .expect("set");
        }
        // WHEN on lit le BLOB brut directement en SQL.
        let conn = Connection::open(&db).expect("open raw");
        let blob: Vec<u8> = conn
            .query_row(
                "SELECT value_encrypted FROM tool_credentials WHERE tool_name='web_search' AND key_name='brave.api_key'",
                params![],
                |r| r.get(0),
            )
            .expect("read blob");
        // THEN la marque ne doit jamais apparaître en clair.
        assert!(
            !String::from_utf8_lossy(&blob).contains("PLAINTEXT-MARKER-XYZ"),
            "ciphertext must not leak the plaintext"
        );
        assert!(blob.len() > 12, "blob must contain nonce + ciphertext");
    }

    #[test]
    fn test_credential_delete_and_list() {
        // GIVEN deux credentials enregistrées.
        let dir = TempDir::new().expect("tempdir");
        let (db, kf) = fresh(&dir);
        let mut store = ToolCredentialStore::new(&db, &kf).expect("open");
        store.set("web_search", "brave.api_key", "v1").expect("a");
        store.set("http_fetch", "auth.token", "v2").expect("b");

        // WHEN on liste filtré, puis on supprime, puis on liste à nouveau.
        let only_search = store.list(Some("web_search")).expect("list filtered");
        assert_eq!(only_search.len(), 1);
        assert_eq!(only_search[0].key_name, "brave.api_key");

        let removed = store
            .delete("http_fetch", "auth.token")
            .expect("delete existing");
        assert!(removed);
        let missing = store
            .delete("http_fetch", "auth.token")
            .expect("delete missing");
        assert!(!missing);

        let all = store.list(None).expect("list all");
        assert_eq!(all.len(), 1);
    }

    #[test]
    fn test_snapshot_reads_disabled_and_credential() {
        // GIVEN une base avec bash_executor désactivé et une clé Brave stockée.
        let dir = TempDir::new().expect("tempdir");
        let (db, kf) = fresh(&dir);
        {
            let mut reg = ToolRegistry::new(&db).expect("open reg");
            reg.set_enabled("bash_executor", false).expect("disable");
            let mut store = ToolCredentialStore::new(&db, &kf).expect("open store");
            store
                .set("web_search", "brave.api_key", "BSA-snapshot")
                .expect("set");
        }
        // WHEN on charge le snapshot.
        let snap = load_governance_snapshot(dir.path()).expect("snapshot");
        // THEN l'outil désactivé et la clé Brave apparaissent.
        assert!(snap.disabled_tools.iter().any(|n| n == "bash_executor"));
        assert_eq!(snap.brave_api_key.as_deref(), Some("BSA-snapshot"));
    }
}
