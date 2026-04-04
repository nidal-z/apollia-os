//! `apollia workspace` — inspecte et initialise le workspace courant.
//!
//! Deux sous-commandes :
//! - `status` : affiche branche git, fichiers modifiés, présence de APOLLIA.md et comptage de fichiers.
//! - `init`   : crée APOLLIA.md avec le template standard (échoue si le fichier existe, sauf `--force`).

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use serde::Serialize;
use thiserror::Error;

use apollia_workspace::git::GitContextCollector;

use crate::exit_codes;

/// Template injecté par `apollia workspace init`.
const APOLLIA_MD_TEMPLATE: &str = "# APOLLIA.md — Instructions pour les agents IA\n\
\n\
## Contexte du projet\n\
<!-- Décrivez ici votre projet : objectif, stack technique, contraintes. -->\n\
\n\
## Règles pour les agents\n\
<!-- Conventions de code, style, règles de commit, etc. -->\n\
\n\
## Fichiers importants\n\
<!-- Liste des fichiers/répertoires clés que l'agent doit connaître. -->\n\
\n\
## Commandes utiles\n\
<!-- Exemples : `cargo test`, `make build`, `npm run dev`. -->\n";

/// Sous-commande `apollia workspace` avec deux actions.
#[derive(Debug, clap::Subcommand)]
pub enum WorkspaceCommand {
    /// Affiche le statut du workspace courant.
    Status,
    /// Initialise APOLLIA.md dans le répertoire courant.
    Init {
        /// Écraser APOLLIA.md s'il existe déjà.
        #[arg(long, default_value_t = false)]
        force: bool,
    },
}

/// Erreurs de la sous-commande workspace.
#[derive(Debug, Error)]
pub enum WorkspaceCliError {
    /// APOLLIA.md existe déjà et `--force` n'a pas été passé.
    #[error("APOLLIA.md existe déjà. Utilisez --force pour écraser.")]
    FileExists,
    /// Erreur de lecture/écriture du système de fichiers.
    #[error("erreur I/O : {0}")]
    Io(#[from] std::io::Error),
}

/// Données de statut du workspace, sérialisables en JSON.
#[derive(Debug, Serialize)]
struct WorkspaceStatusOutput {
    workspace_root: String,
    git_branch: Option<String>,
    modified_files: Vec<String>,
    apollia_md_found: bool,
    file_count: usize,
}

/// Point d'entrée de la sous-commande `apollia workspace`.
pub async fn run(cmd: &WorkspaceCommand, json: bool) -> i32 {
    match cmd {
        WorkspaceCommand::Status => run_workspace_status(json).await,
        WorkspaceCommand::Init { force } => run_workspace_init(*force, json).await,
    }
}

/// Affiche le statut du workspace courant (branche git, fichiers modifiés, APOLLIA.md, comptage).
async fn run_workspace_status(json: bool) -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error: impossible d'obtenir le répertoire courant : {e}");
            return exit_codes::GENERAL_ERROR;
        }
    };

    let status = collect_workspace_status(&cwd).await;

    if json {
        match serde_json::to_string_pretty(&status) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("Error: sérialisation JSON : {e}");
                return exit_codes::GENERAL_ERROR;
            }
        }
    } else {
        print_workspace_status(&status);
    }

    exit_codes::SUCCESS
}

/// Initialise APOLLIA.md dans le répertoire courant.
async fn run_workspace_init(force: bool, json: bool) -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error: impossible d'obtenir le répertoire courant : {e}");
            return exit_codes::GENERAL_ERROR;
        }
    };

    match init_apollia_md(&cwd, force).await {
        Ok(path) => {
            if json {
                let output = serde_json::json!({
                    "path": path.display().to_string(),
                    "created": true,
                });
                println!(
                    "{}",
                    serde_json::to_string_pretty(&output).unwrap_or_default()
                );
            } else {
                println!("APOLLIA.md créé : {}", path.display());
            }
            exit_codes::SUCCESS
        }
        Err(e) => {
            eprintln!("Error: {e}");
            exit_codes::GENERAL_ERROR
        }
    }
}

/// Collecte les données de statut du workspace pour `cwd`.
///
/// Orchestre la collecte git, le comptage de fichiers et la détection de APOLLIA.md.
async fn collect_workspace_status(cwd: &Path) -> WorkspaceStatusOutput {
    let git_result = GitContextCollector::collect(cwd, 200).await;
    let modified_files = parse_modified_files(git_result.status.as_deref());
    let apollia_md_found = cwd.join("APOLLIA.md").exists();
    let file_count = count_files(cwd).await;

    WorkspaceStatusOutput {
        workspace_root: cwd.display().to_string(),
        git_branch: git_result.branch,
        modified_files,
        apollia_md_found,
        file_count,
    }
}

/// Écrit le template APOLLIA.md dans `cwd`.
///
/// Retourne `Err(WorkspaceCliError::FileExists)` si le fichier existe déjà et `force` est `false`.
async fn init_apollia_md(cwd: &Path, force: bool) -> Result<PathBuf, WorkspaceCliError> {
    let target = cwd.join("APOLLIA.md");
    if target.exists() && !force {
        return Err(WorkspaceCliError::FileExists);
    }
    tokio::fs::write(&target, APOLLIA_MD_TEMPLATE).await?;
    Ok(target)
}

/// Affiche le statut du workspace en format humain.
fn print_workspace_status(s: &WorkspaceStatusOutput) {
    println!("Workspace : {}", s.workspace_root);
    match &s.git_branch {
        Some(branch) => println!("Branch git: {branch}"),
        None => println!("Branch git: (hors dépôt git)"),
    }
    if s.modified_files.is_empty() {
        println!("Fichiers modifiés : (aucun)");
    } else {
        println!("Fichiers modifiés :");
        for f in &s.modified_files {
            println!("  {f}");
        }
    }
    println!(
        "APOLLIA.md : {}",
        if s.apollia_md_found {
            "trouvé"
        } else {
            "absent"
        }
    );
    println!("Fichiers : {}", s.file_count);
}

/// Extrait la liste des chemins modifiés depuis la sortie de `git status --short`.
///
/// Format attendu par ligne : `XY PATH` (X = statut index, Y = statut working tree, PATH = chemin).
fn parse_modified_files(status_output: Option<&str>) -> Vec<String> {
    let Some(output) = status_output else {
        return Vec::new();
    };
    output
        .lines()
        .filter_map(|line| {
            if line.len() < 3 {
                return None;
            }
            let path = line[3..].trim();
            if path.is_empty() {
                None
            } else {
                Some(path.to_owned())
            }
        })
        .collect()
}

/// Compte les fichiers récursivement dans `root` en ignorant les répertoires non pertinents.
///
/// Ignore : `.git`, `target`, `node_modules`, `__pycache__`, `dist`, `.next`, `.DS_Store`.
async fn count_files(root: &Path) -> usize {
    let mut count: usize = 0;
    let mut queue: VecDeque<PathBuf> = VecDeque::new();
    queue.push_back(root.to_owned());

    while let Some(dir) = queue.pop_front() {
        let mut rd = match tokio::fs::read_dir(&dir).await {
            Ok(rd) => rd,
            Err(_) => continue,
        };
        while let Ok(Some(entry)) = rd.next_entry().await {
            let name = entry.file_name();
            if should_ignore_for_count(&name.to_string_lossy()) {
                continue;
            }
            let is_dir = entry
                .file_type()
                .await
                .map(|ft| ft.is_dir())
                .unwrap_or(false);
            if is_dir {
                queue.push_back(entry.path());
            } else {
                count += 1;
            }
        }
    }
    count
}

/// Retourne `true` si le nom d'entrée doit être ignoré dans le comptage de fichiers.
fn should_ignore_for_count(name: &str) -> bool {
    matches!(
        name,
        ".git" | "target" | "node_modules" | "__pycache__" | ".DS_Store" | ".next" | "dist"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn workspace_status_shows_git_branch() {
        // GIVEN le dépôt apollia courant (un dépôt git valide)
        let cwd = std::env::current_dir().expect("current_dir");
        // WHEN on collecte le statut
        let status = collect_workspace_status(&cwd).await;
        // THEN la branche git est détectée
        assert!(
            status.git_branch.is_some(),
            "doit détecter la branche dans un dépôt git"
        );
    }

    #[tokio::test]
    async fn workspace_init_creates_apollia_md() {
        // GIVEN répertoire temporaire sans APOLLIA.md
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(!dir.path().join("APOLLIA.md").exists());
        // WHEN init sans --force dans un répertoire vide
        let result = init_apollia_md(dir.path(), false).await;
        // THEN fichier créé avec succès
        assert!(
            result.is_ok(),
            "init doit réussir si le fichier n'existe pas"
        );
        assert!(
            dir.path().join("APOLLIA.md").exists(),
            "APOLLIA.md doit être créé"
        );
    }

    #[tokio::test]
    async fn workspace_init_fails_if_exists_without_force() {
        // GIVEN APOLLIA.md existant
        let dir = tempfile::tempdir().expect("tempdir");
        tokio::fs::write(dir.path().join("APOLLIA.md"), "existing")
            .await
            .expect("write");
        // WHEN init sans --force
        let result = init_apollia_md(dir.path(), false).await;
        // THEN Err(FileExists)
        assert!(
            matches!(result, Err(WorkspaceCliError::FileExists)),
            "doit retourner FileExists sans --force"
        );
    }

    #[tokio::test]
    async fn workspace_init_force_overwrites() {
        // GIVEN APOLLIA.md existant avec un contenu arbitraire
        let dir = tempfile::tempdir().expect("tempdir");
        tokio::fs::write(dir.path().join("APOLLIA.md"), "ancien contenu")
            .await
            .expect("write");
        // WHEN init --force
        let result = init_apollia_md(dir.path(), true).await;
        // THEN succès et contenu du template standard
        assert!(result.is_ok(), "init --force doit réussir");
        let content = tokio::fs::read_to_string(dir.path().join("APOLLIA.md"))
            .await
            .expect("read");
        assert!(
            content.contains("APOLLIA.md"),
            "le template doit être présent après --force : {content}"
        );
        assert!(
            !content.contains("ancien contenu"),
            "l'ancien contenu doit avoir été effacé"
        );
    }

    #[tokio::test]
    async fn workspace_status_json_output() {
        // GIVEN le dépôt apollia courant
        let cwd = std::env::current_dir().expect("current_dir");
        // WHEN on sérialise le statut en JSON
        let status = collect_workspace_status(&cwd).await;
        let json_str = serde_json::to_string_pretty(&status).expect("serialize");
        // THEN le JSON est valide et contient tous les champs requis
        let parsed: serde_json::Value = serde_json::from_str(&json_str).expect("parse");
        assert!(
            parsed["workspace_root"].is_string(),
            "workspace_root manquant"
        );
        assert!(
            parsed["git_branch"].is_string() || parsed["git_branch"].is_null(),
            "git_branch doit être string ou null"
        );
        assert!(
            parsed["modified_files"].is_array(),
            "modified_files doit être un tableau"
        );
        assert!(
            parsed["apollia_md_found"].is_boolean(),
            "apollia_md_found doit être boolean"
        );
        assert!(
            parsed["file_count"].is_number(),
            "file_count doit être un nombre"
        );
    }

    #[test]
    fn parse_modified_files_extracts_paths() {
        // GIVEN sortie git status --short avec trois entrées
        let status = " M src/main.rs\n?? Cargo.toml\n M crates/foo/bar.rs";
        // WHEN
        let files = parse_modified_files(Some(status));
        // THEN trois chemins extraits
        assert_eq!(files.len(), 3, "doit extraire 3 fichiers");
        assert!(files.contains(&"src/main.rs".to_owned()));
        assert!(files.contains(&"Cargo.toml".to_owned()));
        assert!(files.contains(&"crates/foo/bar.rs".to_owned()));
    }

    #[test]
    fn parse_modified_files_returns_empty_for_none() {
        // GIVEN pas de statut git (hors dépôt)
        // WHEN / THEN
        assert!(
            parse_modified_files(None).is_empty(),
            "None doit retourner une liste vide"
        );
    }

    #[tokio::test]
    async fn count_files_ignores_git_and_target() {
        // GIVEN répertoire avec .git/, target/, et src/main.rs
        let dir = tempfile::tempdir().expect("tempdir");
        tokio::fs::create_dir(dir.path().join(".git"))
            .await
            .expect("mkdir .git");
        tokio::fs::create_dir(dir.path().join("target"))
            .await
            .expect("mkdir target");
        tokio::fs::create_dir(dir.path().join("src"))
            .await
            .expect("mkdir src");
        tokio::fs::write(dir.path().join("src").join("main.rs"), "fn main(){}")
            .await
            .expect("write main.rs");
        tokio::fs::write(dir.path().join(".git").join("config"), "git config")
            .await
            .expect("write git config");
        tokio::fs::write(dir.path().join("target").join("out"), "binary")
            .await
            .expect("write target out");
        // WHEN
        let count = count_files(dir.path()).await;
        // THEN seul src/main.rs est compté
        assert_eq!(
            count, 1,
            ".git et target doivent être ignorés : got {count}"
        );
    }
}
