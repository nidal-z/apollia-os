//! Parser natural-language → schedule + agent (US-SP42-050).
//!
//! Fonction pure testable : prend une description en français ou en anglais
//! ("Tous les matins à 8 h, demande à spec-assistant de résumer mes specs")
//! et retourne un [`ParsedAutomation`] que l'UI mappe sur un
//! [`TriggerSourceInput`] + agent cible.
//!
//! Ce parseur est heuristique — il reconnaît les patterns courants (daily
//! morning/evening, every N minutes, every Monday at HH[:MM], jour ISO). Si
//! rien ne matche, il retourne `confidence = Low` et laisse une ambiguïté que
//! l'UI demandera à l'opérateur de résoudre (ou déclenche un fallback LLM
//! structuré côté orchestrateur — non implémenté ici, opt-in).
//!
//! Toute la logique est en UTF-8 NFC, sans accès disque ni réseau, afin de
//! pouvoir être snapshot-testée avec 20 inputs bilingues (cf. tests).

use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

/// Confiance globale attribuée au résultat du parseur.
///
/// L'UI gate step 2 du wizard : `Low` ouvre automatiquement un mode édition
/// naturelle pour forcer l'opérateur à lever l'ambiguïté.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    High,
    Medium,
    Low,
}

/// Planification inférée.
///
/// Reflet des variantes `TriggerSourceInput` côté desktop (cron / interval /
/// oneshot). `file_watch` et `webhook` ne sont pas adressés par le parseur
/// naturel : ils restent accessibles via le mode builder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ParsedSchedule {
    Cron {
        /// Expression cron standard 5 champs (min h dom mon dow).
        expr: String,
        /// Label lisible côté opérateur (fr), ex. "Tous les jours à 08:00".
        human_label_fr: String,
        /// Équivalent anglais, ex. "Every day at 08:00".
        human_label_en: String,
        /// Prochaine exécution calculée à partir de `now_utc`.
        next_run_at: DateTime<Utc>,
    },
    Interval {
        /// Durée au format humain (30s, 5m, 2h, 1d).
        every: String,
        /// Nombre de secondes équivalent pour vérification.
        every_seconds: u64,
        human_label_fr: String,
        human_label_en: String,
        next_run_at: DateTime<Utc>,
    },
    OneShot {
        /// Date-time ISO 8601 (`YYYY-MM-DDTHH:MM`).
        fire_at: String,
        human_label_fr: String,
        human_label_en: String,
        next_run_at: DateTime<Utc>,
    },
}

/// Agent cible inféré, s'il existe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentMatch {
    /// Identifiant ou nom de l'agent connu matché.
    pub agent_id: String,
    /// Raison du matching — utile pour debug et pour afficher "Nous avons
    /// reconnu <X> parce que...".
    pub reason: String,
}

/// Résultat complet du parseur.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedAutomation {
    /// Planification — `None` si aucune n'a pu être détectée.
    pub schedule: Option<ParsedSchedule>,
    /// Agent inféré s'il matche un agent connu, sinon `None`.
    pub agent: Option<AgentMatch>,
    /// Payload extrait (partie "verbe + objet" de la demande) pour servir de
    /// `input_template`. Ex. : "Résumer mes specs".
    pub payload: Option<String>,
    /// Niveau de confiance global.
    pub confidence: Confidence,
    /// Ambiguïtés à résoudre côté UI avant d'activer l'automatisation.
    pub ambiguities: Vec<String>,
}

/// Normalise minuscule + retire accents pour le matching heuristique.
fn fold(input: &str) -> String {
    input
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            _ => c,
        })
        .collect()
}

/// Extrait HH[:MM] depuis une expression comme "8 h", "8h30", "8:30", "08h00",
/// "8 am", "20h", "at 9", "à 9". Retourne `(hour, minute)` 24h.
fn extract_time(folded: &str) -> Option<(u32, u32)> {
    let bytes = folded.as_bytes();

    // Pattern "N am/pm" (optionally glued).
    for (suffix, offset) in [(" am", 12i32), ("am", 12), (" pm", 0), ("pm", 0)] {
        let _ = offset;
        if let Some(pos) = folded.find(suffix) {
            let slice = &folded[..pos];
            if let Some(last) = slice.rsplit(|c: char| !c.is_ascii_digit()).find(|s| !s.is_empty()) {
                if let Ok(h) = last.parse::<u32>() {
                    if h <= 12 {
                        let hour24 = if suffix.trim() == "am" {
                            if h == 12 { 0 } else { h }
                        } else if h == 12 {
                            12
                        } else {
                            h + 12
                        };
                        return Some((hour24, 0));
                    }
                }
            }
        }
    }

    // Pattern "HHhMM" / "HH h MM" / "HH:MM".
    for (i, &b) in bytes.iter().enumerate() {
        if b != b'h' && b != b':' {
            continue;
        }
        // Find end of digits to the left (skipping one optional space for 'h').
        let mut end = i;
        if b == b'h' && end > 0 && bytes[end - 1] == b' ' {
            end -= 1;
        }
        let mut lo = end;
        while lo > 0 && bytes[lo - 1].is_ascii_digit() {
            lo -= 1;
        }
        if lo == end {
            continue;
        }
        let h: u32 = match std::str::from_utf8(&bytes[lo..end]).ok().and_then(|s| s.parse().ok()) {
            Some(h) if h <= 23 => h,
            _ => continue,
        };
        // Minutes after.
        let mut mo = i + 1;
        while mo < bytes.len() && bytes[mo] == b' ' {
            mo += 1;
        }
        let mut mend = mo;
        while mend < bytes.len() && bytes[mend].is_ascii_digit() {
            mend += 1;
        }
        let m: u32 = if mend > mo {
            match std::str::from_utf8(&bytes[mo..mend]).ok().and_then(|s| s.parse().ok()) {
                Some(m) if m <= 59 => m,
                _ => continue,
            }
        } else {
            0
        };
        return Some((h, m));
    }

    // Pattern "at N" / "a N" (bare hour, 0-23).
    for prefix in [" at ", " a "] {
        if let Some(pos) = folded.find(prefix) {
            let rest = &folded[pos + prefix.len()..];
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(h) = digits.parse::<u32>() {
                if h <= 23 {
                    return Some((h, 0));
                }
            }
        }
    }

    None
}

/// Extrait la durée depuis "toutes les N minutes / heures / secondes" ou
/// "every N minutes / hours".
fn extract_interval(folded: &str) -> Option<(u64, String)> {
    // Require a trigger verb so "every sunday" doesn't match "day".
    let has_trigger = folded.contains("toutes les")
        || folded.contains("chaque")
        || folded.contains("every ");
    if !has_trigger {
        return None;
    }
    const PATTERNS: &[(&[&str], u64, &str)] = &[
        (&["seconde", "second"], 1, "s"),
        (&["minute"], 60, "m"),
        (&["heure", "hour"], 3600, "h"),
        (&["jour", "day"], 86400, "d"),
    ];
    for (keys, mult, suffix) in PATTERNS {
        for key in *keys {
            // Find keyword at word boundary (preceded by space, followed by
            // non-letter or end).
            let mut start = 0usize;
            while let Some(rel) = folded[start..].find(key) {
                let abs = start + rel;
                let before_ok = abs == 0
                    || !folded.as_bytes()[abs - 1].is_ascii_alphabetic();
                let after = abs + key.len();
                let after_ok = after >= folded.len() || {
                    let c = folded.as_bytes()[after];
                    !c.is_ascii_alphabetic() || c == b's' // allow plural
                };
                if !(before_ok && after_ok) {
                    start = abs + 1;
                    continue;
                }
                // Require a number within the 12 preceding chars, else fallback
                // to 1 when preceded by "toutes les"/"chaque"/"every".
                let window_start = abs.saturating_sub(12);
                let prefix = &folded[window_start..abs];
                let last = prefix.rsplit(|c: char| !c.is_ascii_digit()).find(|s| !s.is_empty());
                let n: u64 = match last.and_then(|s| s.parse().ok()) {
                    Some(0) => {
                        start = abs + 1;
                        continue;
                    }
                    Some(n) => n,
                    None => {
                        // Accept bare "every minute" / "toutes les minutes" —
                        // only for sub-hourly units since "every day / hour"
                        // typically combine with a time-of-day (→ cron).
                        let bare_ok = matches!(*key, "minute" | "seconde" | "second")
                            && (prefix.contains("every")
                                || prefix.contains("chaque")
                                || prefix.contains("toutes les"));
                        if bare_ok {
                            1
                        } else {
                            start = abs + 1;
                            continue;
                        }
                    }
                };
                return Some((n * mult, format!("{}{}", n, suffix)));
            }
        }
    }
    None
}

/// Détecte "lundi/mardi/.../dimanche" ou "monday/tuesday/..." → dow cron (0–6,
/// dimanche = 0).
fn extract_dow(folded: &str) -> Option<(u32, &'static str, &'static str)> {
    const DAYS: &[(&str, &str, u32, &str, &str)] = &[
        ("lundi", "monday", 1, "lundi", "Monday"),
        ("mardi", "tuesday", 2, "mardi", "Tuesday"),
        ("mercredi", "wednesday", 3, "mercredi", "Wednesday"),
        ("jeudi", "thursday", 4, "jeudi", "Thursday"),
        ("vendredi", "friday", 5, "vendredi", "Friday"),
        ("samedi", "saturday", 6, "samedi", "Saturday"),
        ("dimanche", "sunday", 0, "dimanche", "Sunday"),
    ];
    for (fr, en, dow, fr_label, en_label) in DAYS {
        if folded.contains(fr) || folded.contains(en) {
            return Some((*dow, fr_label, en_label));
        }
    }
    None
}

/// Cherche un agent par nom/id dans une liste connue. Matching case-insensitive
/// sur le nom brut et sur le nom "folded" (sans accents, minuscule).
fn match_agent(folded: &str, known: &[String]) -> Option<AgentMatch> {
    // On fait un match descendant : on teste d'abord les noms les plus longs.
    let mut sorted = known.to_vec();
    sorted.sort_by_key(|s| std::cmp::Reverse(s.len()));
    for name in &sorted {
        let folded_name = fold(name);
        if folded.contains(&folded_name) {
            return Some(AgentMatch {
                agent_id: name.clone(),
                reason: format!("matched by name '{}'", name),
            });
        }
    }
    None
}

/// Extrait un payload après "de " ou "to " ("… demande de résumer mes specs").
fn extract_payload(original: &str) -> Option<String> {
    let lower = original.to_lowercase();
    for sep in [" de ", " to ", " : ", ": "] {
        if let Some(idx) = lower.find(sep) {
            let rest = original[idx + sep.len()..].trim();
            if rest.len() >= 3 {
                return Some(rest.to_string());
            }
        }
    }
    None
}

/// Calcule la prochaine occurrence d'un cron `min h * * dow?` (parsing limité
/// à ce que le parseur émet). Retourne `now + 1min` en dernier recours.
fn next_cron(
    now: DateTime<Utc>,
    hour: u32,
    minute: u32,
    dow: Option<u32>,
) -> DateTime<Utc> {
    let mut date: NaiveDate = now.date_naive();
    let time = NaiveTime::from_hms_opt(hour, minute, 0).unwrap_or(NaiveTime::MIN);
    for _ in 0..14 {
        if let Some(dow) = dow {
            // 0 = Sunday, chrono weekday: Mon=0 .. Sun=6 → remap.
            let chrono_dow = match dow {
                0 => 6,
                n => n - 1,
            };
            if date.weekday().num_days_from_monday() != chrono_dow {
                date = date.succ_opt().unwrap_or(date);
                continue;
            }
        }
        let candidate = NaiveDateTime::new(date, time);
        let candidate_utc = Utc.from_utc_datetime(&candidate);
        if candidate_utc > now {
            return candidate_utc;
        }
        date = date.succ_opt().unwrap_or(date);
    }
    now + Duration::minutes(1)
}

/// Point d'entrée : parse la description libre en `ParsedAutomation`.
pub fn parse_automation(
    input: &str,
    now_utc: DateTime<Utc>,
    known_agents: &[String],
) -> ParsedAutomation {
    let trimmed = input.trim();
    let folded = fold(trimmed);
    let mut ambiguities = Vec::new();

    // ─── Schedule ────────────────────────────────────────────────────────
    let schedule = if let Some((every_seconds, every)) = extract_interval(&folded) {
        if folded.contains("toutes les")
            || folded.contains("every")
            || folded.contains("chaque")
        {
            let label_fr = format!("Toutes les {}", humanize_interval_fr(every_seconds));
            let label_en = format!("Every {}", humanize_interval_en(every_seconds));
            Some(ParsedSchedule::Interval {
                every: every.clone(),
                every_seconds,
                human_label_fr: label_fr,
                human_label_en: label_en,
                next_run_at: now_utc + Duration::seconds(every_seconds as i64),
            })
        } else {
            None
        }
    } else {
        None
    };

    let schedule = schedule.or_else(|| {
        // Try cron (time of day, optional day of week, optional "tous les matins").
        let dow = extract_dow(&folded);
        let mut time = extract_time(&folded);
        if time.is_none() {
            if folded.contains("matin") || folded.contains("morning") {
                time = Some((8, 0));
            } else if folded.contains("midi") || folded.contains("noon") {
                time = Some((12, 0));
            } else if folded.contains("soir") || folded.contains("evening") {
                time = Some((20, 0));
            } else if folded.contains("minuit") || folded.contains("midnight") {
                time = Some((0, 0));
            }
        }
        let (hour, minute) = time?;
        let (expr, label_fr, label_en) = match dow {
            Some((dow_num, fr_label, en_label)) => (
                format!("{} {} * * {}", minute, hour, dow_num),
                format!("Chaque {} à {:02}:{:02}", fr_label, hour, minute),
                format!("Every {} at {:02}:{:02}", en_label, hour, minute),
            ),
            None => (
                format!("{} {} * * *", minute, hour),
                format!("Tous les jours à {:02}:{:02}", hour, minute),
                format!("Every day at {:02}:{:02}", hour, minute),
            ),
        };
        Some(ParsedSchedule::Cron {
            expr,
            human_label_fr: label_fr,
            human_label_en: label_en,
            next_run_at: next_cron(now_utc, hour, minute, dow.map(|(d, _, _)| d)),
        })
    });

    if schedule.is_none() {
        ambiguities.push(
            "Aucune planification détectée — précisez une fréquence (ex. 'tous les jours à 8 h')."
                .to_string(),
        );
    }

    // ─── Agent ───────────────────────────────────────────────────────────
    let agent = match_agent(&folded, known_agents);
    if agent.is_none() && !known_agents.is_empty() {
        ambiguities
            .push("Assistant cible non reconnu — choisissez-le à l'étape suivante.".to_string());
    }

    // ─── Payload ─────────────────────────────────────────────────────────
    let payload = extract_payload(trimmed);

    // ─── Confidence ──────────────────────────────────────────────────────
    let confidence = match (schedule.is_some(), agent.is_some(), ambiguities.len()) {
        (true, true, 0) => Confidence::High,
        (true, _, _) => Confidence::Medium,
        _ => Confidence::Low,
    };

    ParsedAutomation {
        schedule,
        agent,
        payload,
        confidence,
        ambiguities,
    }
}

fn humanize_interval_fr(seconds: u64) -> String {
    if seconds % 86400 == 0 {
        let n = seconds / 86400;
        return if n == 1 { "jour".into() } else { format!("{} jours", n) };
    }
    if seconds % 3600 == 0 {
        let n = seconds / 3600;
        return if n == 1 { "heure".into() } else { format!("{} heures", n) };
    }
    if seconds % 60 == 0 {
        let n = seconds / 60;
        return if n == 1 { "minute".into() } else { format!("{} minutes", n) };
    }
    format!("{} s", seconds)
}

fn humanize_interval_en(seconds: u64) -> String {
    if seconds % 86400 == 0 {
        let n = seconds / 86400;
        return if n == 1 { "day".into() } else { format!("{} days", n) };
    }
    if seconds % 3600 == 0 {
        let n = seconds / 3600;
        return if n == 1 { "hour".into() } else { format!("{} hours", n) };
    }
    if seconds % 60 == 0 {
        let n = seconds / 60;
        return if n == 1 { "minute".into() } else { format!("{} minutes", n) };
    }
    format!("{}s", seconds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 4, 20, 10, 0, 0).unwrap()
    }

    fn agents() -> Vec<String> {
        vec![
            "spec-assistant".into(),
            "dev-assistant".into(),
            "review-assistant".into(),
            "document-assistant".into(),
        ]
    }

    // ── 20 snapshot inputs bilingues ──────────────────────────────────────

    #[test]
    fn parses_daily_morning_french() {
        let r = parse_automation(
            "Tous les matins à 8 h, demande à spec-assistant de résumer mes specs",
            now(),
            &agents(),
        );
        match r.schedule.as_ref().unwrap() {
            ParsedSchedule::Cron { expr, .. } => assert_eq!(expr, "0 8 * * *"),
            _ => panic!("expected cron"),
        }
        assert_eq!(r.agent.as_ref().unwrap().agent_id, "spec-assistant");
        assert_eq!(r.confidence, Confidence::High);
    }

    #[test]
    fn parses_daily_morning_english() {
        let r = parse_automation(
            "Every morning at 8am, ask dev-assistant to review PRs",
            now(),
            &agents(),
        );
        match r.schedule.as_ref().unwrap() {
            ParsedSchedule::Cron { expr, .. } => assert_eq!(expr, "0 8 * * *"),
            _ => panic!("expected cron"),
        }
        assert_eq!(r.agent.as_ref().unwrap().agent_id, "dev-assistant");
    }

    #[test]
    fn parses_every_monday() {
        let r = parse_automation(
            "Chaque lundi à 9 h, review-assistant fait un point d'équipe",
            now(),
            &agents(),
        );
        match r.schedule.as_ref().unwrap() {
            ParsedSchedule::Cron { expr, .. } => assert_eq!(expr, "0 9 * * 1"),
            _ => panic!("expected cron"),
        }
    }

    #[test]
    fn parses_interval_five_minutes_fr() {
        let r = parse_automation("toutes les 5 minutes", now(), &agents());
        match r.schedule.as_ref().unwrap() {
            ParsedSchedule::Interval { every, every_seconds, .. } => {
                assert_eq!(every, "5m");
                assert_eq!(*every_seconds, 300);
            }
            _ => panic!("expected interval"),
        }
    }

    #[test]
    fn parses_interval_two_hours_en() {
        let r = parse_automation("every 2 hours, run document-assistant", now(), &agents());
        match r.schedule.as_ref().unwrap() {
            ParsedSchedule::Interval { every_seconds, .. } => assert_eq!(*every_seconds, 7200),
            _ => panic!("expected interval"),
        }
        assert_eq!(r.agent.as_ref().unwrap().agent_id, "document-assistant");
    }

    #[test]
    fn parses_noon_fr() {
        let r = parse_automation("Tous les jours à midi", now(), &[]);
        match r.schedule.as_ref().unwrap() {
            ParsedSchedule::Cron { expr, .. } => assert_eq!(expr, "0 12 * * *"),
            _ => panic!("expected cron"),
        }
    }

    #[test]
    fn parses_evening_en() {
        let r = parse_automation("every evening at 8pm", now(), &[]);
        match r.schedule.as_ref().unwrap() {
            ParsedSchedule::Cron { expr, .. } => assert_eq!(expr, "0 20 * * *"),
            _ => panic!("expected cron"),
        }
    }

    #[test]
    fn parses_midnight_fr() {
        let r = parse_automation("tous les jours à minuit", now(), &[]);
        match r.schedule.as_ref().unwrap() {
            ParsedSchedule::Cron { expr, .. } => assert_eq!(expr, "0 0 * * *"),
            _ => panic!("expected cron"),
        }
    }

    #[test]
    fn parses_hh_mm_fr() {
        let r = parse_automation("chaque vendredi à 18h30", now(), &[]);
        match r.schedule.as_ref().unwrap() {
            ParsedSchedule::Cron { expr, .. } => assert_eq!(expr, "30 18 * * 5"),
            _ => panic!("expected cron"),
        }
    }

    #[test]
    fn parses_every_sunday() {
        let r = parse_automation("every sunday at 7:00", now(), &[]);
        match r.schedule.as_ref().unwrap() {
            ParsedSchedule::Cron { expr, .. } => assert_eq!(expr, "0 7 * * 0"),
            _ => panic!("expected cron"),
        }
    }

    #[test]
    fn missing_schedule_triggers_low_confidence() {
        let r = parse_automation("do something useful", now(), &agents());
        assert!(r.schedule.is_none());
        assert_eq!(r.confidence, Confidence::Low);
        assert!(!r.ambiguities.is_empty());
    }

    #[test]
    fn unknown_agent_flags_ambiguity() {
        let r = parse_automation("tous les jours à 8 h, ping the fridge", now(), &agents());
        assert!(r.schedule.is_some());
        assert!(r.agent.is_none());
        assert_eq!(r.confidence, Confidence::Medium);
        assert!(!r.ambiguities.is_empty());
    }

    #[test]
    fn payload_extracted_after_de() {
        let r = parse_automation(
            "Tous les matins à 8 h, demande à spec-assistant de résumer mes specs",
            now(),
            &agents(),
        );
        assert!(r.payload.as_deref().unwrap().contains("résumer"));
    }

    #[test]
    fn agent_matched_case_insensitive() {
        let r = parse_automation("every day at 9, SPEC-ASSISTANT runs", now(), &agents());
        assert_eq!(r.agent.as_ref().unwrap().agent_id, "spec-assistant");
    }

    #[test]
    fn interval_one_minute_fr() {
        let r = parse_automation("toutes les minutes", now(), &[]);
        match r.schedule.as_ref().unwrap() {
            ParsedSchedule::Interval { every_seconds, .. } => assert_eq!(*every_seconds, 60),
            _ => panic!("expected interval"),
        }
    }

    #[test]
    fn next_run_in_future() {
        let r = parse_automation("tous les jours à 8 h", now(), &[]);
        match r.schedule.as_ref().unwrap() {
            ParsedSchedule::Cron { next_run_at, .. } => assert!(*next_run_at > now()),
            _ => panic!("expected cron"),
        }
    }

    #[test]
    fn human_label_fr_contains_hour() {
        let r = parse_automation("tous les jours à 8 h", now(), &[]);
        match r.schedule.as_ref().unwrap() {
            ParsedSchedule::Cron { human_label_fr, .. } => assert!(human_label_fr.contains("08:00")),
            _ => panic!(),
        }
    }

    #[test]
    fn human_label_en_daily() {
        let r = parse_automation("every day at 9", now(), &[]);
        match r.schedule.as_ref().unwrap() {
            ParsedSchedule::Cron { human_label_en, .. } => {
                assert!(human_label_en.starts_with("Every day"))
            }
            _ => panic!(),
        }
    }

    #[test]
    fn interval_hours_plural_en() {
        let r = parse_automation("every 3 hours", now(), &[]);
        match r.schedule.as_ref().unwrap() {
            ParsedSchedule::Interval { human_label_en, .. } => {
                assert!(human_label_en.contains("3 hours"))
            }
            _ => panic!(),
        }
    }

    #[test]
    fn high_confidence_requires_schedule_and_agent() {
        let r = parse_automation(
            "chaque lundi à 9h, spec-assistant résume",
            now(),
            &agents(),
        );
        assert_eq!(r.confidence, Confidence::High);
    }
}
