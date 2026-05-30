//! Runtime error analyzers.
//!
//! - [`error_analyzer`], maps a raw error string into a structured
//!   [`apollia_core::ErrorAnalysis`] using a static lookup table. Always-on,
//!   zero LLM cost.
//! - [`hallucination_detector`], heuristic + schema-driven detector for
//!   suspicious tool outputs (null, empty, malformed JSON). Always-on,
//! (session-level meta-layer).
//!
//! When the static classifier returns [`apollia_core::ErrorCategory::Unknown`]
//! and the user has opted in to `routines.error_explanation`, the runtime
//! invokes `MetaRoutine::GenerateErrorExplanation` to enrich the analysis.
//! See [`error_analyzer::enrich_with_llm`] for the helper.

pub mod confidence_parser;
pub mod error_analyzer;
pub mod hallucination_detector;
pub mod risk_analyzer;

pub use confidence_parser::{
    parse as parse_confidence, Assertion, AssertionRange, Citation, ConfidenceLevel, ParsedMessage,
    SourceType,
};
pub use error_analyzer::{classify_llm_error, classify_tool_error, enrich_with_llm};
pub use hallucination_detector::{
    compute_session_hallucination_risk, detect_hallucination, HallucinationRisk, HeuristicReport,
    SchemaValidator, SessionHallucinationInputs,
};
pub use risk_analyzer::analyze as analyze_risk;
