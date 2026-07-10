#![no_main]

//! Fuzzes GBNF grammar generation, the real `apollia_llm::tool_specs_to_gbnf`.
//! The input derives tool specs whose names, descriptions and `parameters`
//! JSON are attacker-shaped, exercising the escaping path (`lit`, `json_key`,
//! `json_string_oneof`) and the JSON-schema walk. The generator must never
//! panic on any tool set.

use apollia_llm::ToolSpec;
use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

#[derive(Arbitrary, Debug)]
struct SpecSeed {
    name: String,
    description: String,
    params_json: String,
}

#[derive(Arbitrary, Debug)]
struct GbnfInput {
    specs: Vec<SpecSeed>,
}

fuzz_target!(|input: GbnfInput| {
    let specs: Vec<ToolSpec> = input
        .specs
        .into_iter()
        .map(|s| {
            let parameters =
                serde_json::from_str(&s.params_json).unwrap_or(serde_json::Value::Null);
            ToolSpec {
                name: s.name,
                description: s.description,
                parameters,
            }
        })
        .collect();
    let _ = apollia_llm::tool_specs_to_gbnf(&specs);
});
