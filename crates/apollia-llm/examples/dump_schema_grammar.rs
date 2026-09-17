//! Print the GBNF grammar a JSON Schema translates to.
//!
//! Development aid for checking a grammar against a live `llama-server`:
//! `cargo run -p apollia-llm --example dump_schema_grammar -- '<schema json>'`.
fn main() {
    let arg = std::env::args().nth(1).unwrap_or_else(|| "{}".to_string());
    let schema: serde_json::Value = match serde_json::from_str(&arg) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("not JSON: {e}");
            std::process::exit(2);
        }
    };
    match apollia_llm::json_schema_to_gbnf(&schema) {
        Ok(gbnf) => print!("{gbnf}"),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
