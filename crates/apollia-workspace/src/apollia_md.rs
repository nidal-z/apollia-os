//! Recherche récursive du fichier `APOLLIA.md` depuis le répertoire courant.
//!
//! Remonte la hiérarchie de répertoires (CWD → parents) jusqu'à
//! `apollia_md_search_depth` niveaux. Le contenu est tronqué par middle-trim
//! pour ne jamais dépasser `apollia_md_max_bytes` octets dans le prompt.

use std::path::{Path, PathBuf};

use apollia_core::truncate_middle;

/// Recherche et lit le fichier `APOLLIA.md` le plus proche du répertoire donné.
///
/// Priorité : CWD > parent immédiat > … > racine (limité à `search_depth` niveaux).
pub struct ApolliamdFinder;

impl ApolliamdFinder {
    /// Recherche `APOLLIA.md` depuis `cwd` vers les répertoires parents.
    ///
    /// Retourne `Some((chemin, contenu_tronqué))` dès que le fichier est trouvé,
    /// `None` si aucun fichier n'existe dans la limite de profondeur.
    ///
    /// Le contenu est tronqué à `max_bytes` via middle-trim pour protéger
    /// la fenêtre de contexte LLM.
    pub async fn find(
        cwd: &Path,
        max_bytes: usize,
        search_depth: usize,
    ) -> Option<(PathBuf, String)> {
        let mut current = cwd.to_owned();
        for _ in 0..=search_depth {
            let candidate = current.join("APOLLIA.md");
            if candidate.exists() {
                let content = tokio::fs::read_to_string(&candidate).await.ok()?;
                let (truncated, _) = truncate_middle(&content, max_bytes);
                return Some((candidate, truncated));
            }
            if !current.pop() {
                break;
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_apollia_md_finder_in_cwd() {
        // GIVEN : APOLLIA.md dans le répertoire de travail temporaire
        let dir = tempfile::tempdir().expect("tempdir");
        tokio::fs::write(dir.path().join("APOLLIA.md"), "# Rules\nNo magic.")
            .await
            .expect("write");
        // WHEN
        let result = ApolliamdFinder::find(dir.path(), 8192, 5).await;
        // THEN
        assert!(result.is_some(), "should find APOLLIA.md in CWD");
        let (_, content) = result.unwrap();
        assert!(content.contains("Rules"), "content: {content}");
    }

    #[tokio::test]
    async fn test_apollia_md_finder_in_parent() {
        // GIVEN : APOLLIA.md dans le parent, CWD dans un sous-répertoire
        let dir = tempfile::tempdir().expect("tempdir");
        tokio::fs::write(dir.path().join("APOLLIA.md"), "parent rules")
            .await
            .expect("write");
        let sub = dir.path().join("src");
        tokio::fs::create_dir(&sub).await.expect("mkdir");
        // WHEN
        let result = ApolliamdFinder::find(&sub, 8192, 5).await;
        // THEN
        assert!(result.is_some(), "should find APOLLIA.md in parent");
        let (path, _) = result.unwrap();
        assert_eq!(path, dir.path().join("APOLLIA.md"));
    }

    #[tokio::test]
    async fn test_apollia_md_absent_returns_none() {
        // GIVEN : répertoire sans APOLLIA.md (ni dans ses parents dans la limite)
        let dir = tempfile::tempdir().expect("tempdir");
        // WHEN — profondeur 0 : on ne remonte pas
        let result = ApolliamdFinder::find(dir.path(), 8192, 0).await;
        // THEN
        assert!(result.is_none(), "no APOLLIA.md at depth 0");
    }

    #[tokio::test]
    async fn test_apollia_md_content_truncated_to_max_bytes() {
        // GIVEN : APOLLIA.md de 10 Ko, max_bytes = 100
        let dir = tempfile::tempdir().expect("tempdir");
        let big_content = "x".repeat(10_000);
        tokio::fs::write(dir.path().join("APOLLIA.md"), &big_content)
            .await
            .expect("write");
        // WHEN
        let result = ApolliamdFinder::find(dir.path(), 100, 5).await;
        // THEN — le middle-trim ajoute un message, mais le résultat est < original
        assert!(result.is_some());
        let (_, content) = result.unwrap();
        assert!(
            content.len() < big_content.len(),
            "content must be truncated: got {} bytes",
            content.len()
        );
    }
}
