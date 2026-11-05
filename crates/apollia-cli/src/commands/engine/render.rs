//! The human rendering of the engine routes, kept free of any input or output
//! so each case is a test on lines.

use serde_json::Value;

/// One line of output: the requested data, or a line `--quiet` drops.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Line {
    /// The data asked for.
    Data(String),
    /// A header, a spacer or a hint.
    Note(String),
}

impl Line {
    pub(crate) fn print(&self) {
        match self {
            Self::Data(text) => println!("{text}"),
            Self::Note(text) if text.is_empty() => crate::note!(),
            Self::Note(text) => crate::note!("{text}"),
        }
    }

    /// The text of the line, whatever its kind.
    #[cfg(test)]
    pub(crate) fn text(&self) -> &str {
        match self {
            Self::Data(text) | Self::Note(text) => text,
        }
    }
}

/// The sentence shown when no manifest of the channel has been verified yet.
pub(crate) const CATALOGUE_NOT_READ: &str = "The engine catalogue has not been read yet.";
/// What to do about it.
pub(crate) const CHECK_HINT: &str = "Run `apollia-os engine check` to read it.";

fn str_of<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

fn array_of<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key)
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice)
}

/// The major of a driver version, `560` for `560.94`.
fn major(version: &str) -> &str {
    version.split('.').next().unwrap_or(version)
}

/// Why a variant is not recommended, in words, from the reason the route gives.
pub(crate) fn reason_text(reason: &Value) -> String {
    let code = str_of(reason, "code").unwrap_or("");
    match code {
        "driver_too_old" => {
            let have = str_of(reason, "have").unwrap_or("?");
            let need = str_of(reason, "need").unwrap_or("?");
            if major(have) == major(need) {
                format!("driver {have} < {need}")
            } else {
                format!("driver {} < {}", major(have), major(need))
            }
        }
        "arch_dropped_in_cuda13" => format!(
            "compute capability {} is not built by CUDA 13",
            str_of(reason, "cc").unwrap_or("?")
        ),
        "gfx_not_in_build" => format!(
            "{} is not in this build",
            str_of(reason, "gfx").unwrap_or("?")
        ),
        "no_vulkan_loader" => "no Vulkan loader installed".to_owned(),
        "no_nvidia_driver" => "no NVIDIA driver answers".to_owned(),
        "not_built_for_platform" => "not built for this platform".to_owned(),
        "no_gpu" => "no card this build drives".to_owned(),
        "engine_sees_no_device" => "the installed build sees no device".to_owned(),
        "no_rocm_runtime" => "no ROCm runtime on this machine".to_owned(),
        other => other.to_owned(),
    }
}

/// The verdict on one variant, with its reason when one is given.
///
/// ROCm rated compatible without a reason under Windows is the build the
/// upstream project calls experimental there.
pub(crate) fn verdict_text(rating: &Value, windows: bool) -> String {
    let verdict = str_of(rating, "verdict").unwrap_or("unknown");
    match rating.get("reason").filter(|r| !r.is_null()) {
        Some(reason) => format!("{verdict} ({})", reason_text(reason)),
        None if windows && verdict == "compatible" && str_of(rating, "variant") == Some("rocm") => {
            "compatible (experimental on Windows)".to_owned()
        }
        None => verdict.to_owned(),
    }
}

fn card_line(card: &Value) -> String {
    let mut parts = vec![str_of(card, "name").unwrap_or("unnamed card").to_owned()];
    if let Some(driver) = str_of(card, "driver_version") {
        parts.push(format!("driver {driver}"));
    }
    if let Some(vram) = card.get("vram_gb").and_then(Value::as_f64) {
        parts.push(format!("{vram:.1} GiB"));
    }
    if let Some(cc) = str_of(card, "compute_capability") {
        parts.push(format!("compute {cc}"));
    }
    if let Some(gfx) = str_of(card, "gfx_target") {
        parts.push(gfx.to_owned());
    }
    if card.get("timed_out").and_then(Value::as_bool) == Some(true) {
        parts.push("driver did not answer in time".to_owned());
    }
    format!("  {}", parts.join("  "))
}

fn hardware(doc: &Value, out: &mut Vec<Line>) {
    out.push(Line::Note("Hardware".to_owned()));
    match doc.get("hardware").filter(|h| !h.is_null()) {
        None => out.push(Line::Data(
            "  the hardware probe did not answer in time".to_owned(),
        )),
        Some(hw) => {
            let cards = array_of(hw, "cards");
            if cards.is_empty() {
                out.push(Line::Data("  no graphics card detected".to_owned()));
            }
            out.extend(cards.iter().map(|c| Line::Data(card_line(c))));
        }
    }
}

fn installed(doc: &Value, out: &mut Vec<Line>) {
    out.push(Line::Note(String::new()));
    out.push(Line::Note("Installed".to_owned()));
    let engines: Vec<&Value> = array_of(doc, "engines")
        .iter()
        .filter(|e| str_of(e, "engine") != Some("nodejs"))
        .collect();
    if engines.is_empty() {
        out.push(Line::Data("  none".to_owned()));
    }
    for e in engines {
        let mut parts = vec![
            str_of(e, "engine").unwrap_or("?").to_owned(),
            str_of(e, "tag").unwrap_or("-").to_owned(),
            str_of(e, "variant").unwrap_or("-").to_owned(),
            str_of(e, "source").unwrap_or("?").to_owned(),
        ];
        if e.get("in_service").and_then(Value::as_bool) == Some(true) {
            parts.push("in service".to_owned());
        }
        out.push(Line::Data(format!("  {}", parts.join("  "))));
    }
}

fn available(doc: &Value, all: bool, windows: bool, out: &mut Vec<Line>) {
    out.push(Line::Note(String::new()));
    let channel = str_of(doc, "channel").unwrap_or("?");
    out.push(Line::Note(format!("Available on the {channel} channel")));
    let ratings = array_of(doc, "ratings");
    let packages: Vec<&Value> = array_of(doc, "available")
        .iter()
        .filter(|p| str_of(p, "engine") == Some("llama-server"))
        .collect();
    let read = doc.get("manifest_sequence").is_some_and(|s| !s.is_null());
    if !read && ratings.is_empty() && packages.is_empty() {
        out.push(Line::Data(format!("  {CATALOGUE_NOT_READ}")));
        out.push(Line::Note(format!("  {CHECK_HINT}")));
        return;
    }
    let mut hidden = 0usize;
    for rating in ratings {
        let variant = str_of(rating, "variant").unwrap_or("?");
        let offered = packages
            .iter()
            .find(|p| str_of(p, "variant") == Some(variant));
        let incompatible = str_of(rating, "verdict") == Some("incompatible");
        if incompatible && !all {
            hidden += 1;
            continue;
        }
        if offered.is_none() && !incompatible {
            continue;
        }
        let tag = offered.and_then(|p| str_of(p, "tag")).unwrap_or("-");
        let mut line = format!("  {variant}  {tag}  {}", verdict_text(rating, windows));
        if offered
            .and_then(|p| p.get("installed"))
            .and_then(Value::as_bool)
            == Some(true)
        {
            line.push_str("  installed");
        }
        out.push(Line::Data(line));
    }
    if ratings.is_empty() {
        // The cards are not known yet: the packages are listed unrated.
        for p in &packages {
            out.push(Line::Data(format!(
                "  {}  {}  not rated (hardware unknown)",
                str_of(p, "variant").unwrap_or("?"),
                str_of(p, "tag").unwrap_or("-"),
            )));
        }
    }
    if hidden > 0 {
        out.push(Line::Note(format!(
            "  {hidden} variant(s) that do not run here; `--all` lists them"
        )));
    }
}

fn nodejs(doc: &Value, out: &mut Vec<Line>) {
    out.push(Line::Note(String::new()));
    out.push(Line::Note("Node.js".to_owned()));
    let installed = array_of(doc, "engines")
        .iter()
        .find(|e| str_of(e, "engine") == Some("nodejs"));
    let offered = array_of(doc, "available")
        .iter()
        .find(|p| str_of(p, "engine") == Some("nodejs"));
    let line = match (installed, offered) {
        (Some(e), _) => format!(
            "  installed  {}  {}",
            str_of(e, "tag").unwrap_or("-"),
            str_of(e, "version").unwrap_or("")
        )
        .trim_end()
        .to_owned(),
        (None, Some(p)) => format!(
            "  not installed; {} on the channel",
            str_of(p, "tag").unwrap_or("-")
        ),
        (None, None) => "  not installed".to_owned(),
    };
    out.push(Line::Data(line));
}

/// `engine list` from the body of `GET /api/v1/engines`.
pub(crate) fn list(doc: &Value, all: bool, windows: bool) -> Vec<Line> {
    let mut out = Vec::new();
    hardware(doc, &mut out);
    installed(doc, &mut out);
    available(doc, all, windows, &mut out);
    nodejs(doc, &mut out);
    out
}

/// `engine check` from the body of `POST /api/v1/engines/check`.
pub(crate) fn check(doc: &Value) -> Vec<Line> {
    let channel = str_of(doc, "channel").unwrap_or("?");
    let sequence = doc
        .get("manifest_sequence")
        .and_then(Value::as_u64)
        .map_or_else(|| "?".to_owned(), |s| s.to_string());
    let mut out = vec![Line::Data(format!(
        "{channel} channel verified, manifest {sequence}"
    ))];
    for u in array_of(doc, "updates") {
        if u.get("update_available").and_then(Value::as_bool) != Some(true) {
            continue;
        }
        out.push(Line::Data(format!(
            "  update: {} {} {} -> {}",
            str_of(u, "engine").unwrap_or("?"),
            str_of(u, "variant").unwrap_or("?"),
            str_of(u, "installed_tag").unwrap_or("-"),
            str_of(u, "latest_tag").unwrap_or("?"),
        )));
    }
    out
}

/// What `engine use` makes of a choice against the engines installed.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ChoiceVerdict {
    /// No build of that variant is installed.
    NotInstalled,
    /// Written, with a warning when the build is rated incompatible.
    Accepted {
        /// Why the build may not run here.
        warning: Option<String>,
    },
}

/// Judge `choice` against the body of `GET /api/v1/engines`.
///
/// `auto` and `embedded` are always accepted. `cuda` names the best CUDA
/// build installed, so either CUDA variant answers for it. A named variant is
/// a build of the store, downloaded or imported, never the embedded engine.
pub(crate) fn judge_choice(doc: &Value, choice: &str) -> ChoiceVerdict {
    if matches!(choice, "auto" | "embedded") {
        return ChoiceVerdict::Accepted { warning: None };
    }
    let wanted: &[&str] = if choice == "cuda" {
        &["cuda-12", "cuda-13"]
    } else {
        std::slice::from_ref(&choice)
    };
    let held: Vec<&str> = array_of(doc, "engines")
        .iter()
        .filter(|e| str_of(e, "engine") == Some("llama-server"))
        .filter(|e| str_of(e, "source") != Some("embedded"))
        .filter_map(|e| str_of(e, "variant"))
        .filter(|v| wanted.contains(v))
        .collect();
    if held.is_empty() {
        return ChoiceVerdict::NotInstalled;
    }
    let incompatible: Vec<&Value> = array_of(doc, "ratings")
        .iter()
        .filter(|r| str_of(r, "variant").is_some_and(|v| held.contains(&v)))
        .filter(|r| str_of(r, "verdict") == Some("incompatible"))
        .collect();
    let all_incompatible = !incompatible.is_empty()
        && held.iter().all(|v| {
            incompatible
                .iter()
                .any(|r| str_of(r, "variant") == Some(*v))
        });
    let warning = all_incompatible.then(|| {
        let why = incompatible
            .first()
            .and_then(|r| r.get("reason"))
            .filter(|r| !r.is_null())
            .map(reason_text)
            .unwrap_or_else(|| "rated incompatible".to_owned());
        format!("the {choice} build is rated incompatible with this machine ({why})")
    });
    ChoiceVerdict::Accepted { warning }
}
