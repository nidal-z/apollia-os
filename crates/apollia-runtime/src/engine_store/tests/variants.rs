//! The names of the variants: two CUDA runtimes and ROCm, the former `cuda`
//! read as CUDA 12, and a manifest that names a variant this build does not
//! know.

use std::sync::{Arc, Mutex};

use super::*;

/// Records the fields of every event it sees, one map per event.
#[derive(Clone, Default)]
pub(super) struct Recorder {
    pub(super) events: Arc<Mutex<Vec<HashMap<String, String>>>>,
}

struct Fields<'a>(&'a mut HashMap<String, String>);

impl tracing::field::Visit for Fields<'_> {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.0.insert(field.name().to_owned(), value.to_owned());
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.0.insert(
            field.name().to_owned(),
            format!("{value:?}").trim_matches('"').to_owned(),
        );
    }
}

impl tracing::Subscriber for Recorder {
    fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        let mut fields = HashMap::new();
        event.record(&mut Fields(&mut fields));
        self.events.lock().unwrap().push(fields);
    }
    fn enter(&self, _: &tracing::span::Id) {}
    fn exit(&self, _: &tracing::span::Id) {}
}

fn package(engine: &str, variant: &str, url: &str) -> serde_json::Value {
    serde_json::json!({
        "engine": engine, "tag": "b7000", "variant": variant,
        "platform": "linux-x64", "size": 10, "url": url, "sha256": "a".repeat(64),
    })
}

#[test]
fn test_the_former_cuda_name_reads_as_cuda_12_and_each_new_name_round_trips() {
    // GIVEN the former name of the CUDA build and the names of the new builds
    let former = "\"cuda\"";

    // WHEN each is read
    let read = serde_json::from_str::<Variant>(former).unwrap();

    // THEN the former name is CUDA 12, and every variant writes and reads
    // back its dashed name
    assert_eq!(read, Variant::Cuda12);
    assert_eq!(Variant::from_wire("cuda"), Some(Variant::Cuda12));
    for (variant, name) in [
        (Variant::Cuda12, "cuda-12"),
        (Variant::Cuda13, "cuda-13"),
        (Variant::Rocm, "rocm"),
    ] {
        assert_eq!(serde_json::to_value(variant).unwrap(), name);
        assert_eq!(
            serde_json::from_value::<Variant>(serde_json::json!(name)).unwrap(),
            variant
        );
        assert_eq!(Variant::from_wire(name), Some(variant));
    }
    assert_eq!(Variant::from_wire("sycl"), None);
}

#[test]
fn test_a_build_an_earlier_version_filed_under_cuda_is_listed_as_cuda_12() {
    // GIVEN a store where an earlier version filed a CUDA build under the
    // directory `cuda`, with the record it wrote then
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let filed = root.join("llama-server").join("b6000").join("cuda");
    std::fs::create_dir_all(filed.join("build").join("bin")).unwrap();
    std::fs::write(
        filed.join("build").join("bin").join("llama-server"),
        b"binary",
    )
    .unwrap();
    let record = r#"{"engine":"llama-server","tag":"b6000","variant":"cuda","platform":"linux-x64","sha256":"0000000000000000000000000000000000000000000000000000000000000000","source":"downloaded"}"#;
    std::fs::write(filed.join(RECORD_FILE), record).unwrap();

    // WHEN the store is listed and the launcher looks for its CUDA build
    let listed = installed_packages(root);
    let newest = newest_installed(root, Engine::LlamaServer, Variant::Cuda12);
    let taken = preferred_llama_server(root, "llama-server", nvidia);

    // THEN the build is CUDA 12, where it was filed, and its binary is found
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0].variant, Variant::Cuda12);
    assert_eq!(listed[0].path, filed);
    let newest = newest.unwrap();
    assert_eq!(newest.tag, "b6000");
    assert_eq!(
        taken,
        Some(filed.join("build").join("bin").join("llama-server"))
    );
    assert_eq!(
        find_binary(&newest.path, "llama-server"),
        Some(filed.join("build").join("bin").join("llama-server"))
    );
}

#[test]
fn test_a_package_of_an_unknown_variant_or_engine_is_left_out_and_logged() {
    // GIVEN a manifest that publishes a variant and an engine this build does
    // not know beside a CUDA 12 build it does
    let manifest = serde_json::json!({
        "format": 1, "channel": "stable", "sequence": 7,
        "packages": [
            package("llama-server", "sycl", "sycl.zip"),
            package("whisper-server", "cpu", "whisper.zip"),
            package("llama-server", "cuda-12", "cuda.zip"),
        ],
    });
    let bytes = serde_json::to_vec(&manifest).unwrap();
    let recorder = Recorder::default();

    // WHEN it is parsed
    let parsed = {
        let _guard = crate::test_support::recording(recorder.clone());
        Manifest::parse(&bytes)
    };

    // THEN it is accepted with the known package only, and each package left
    // out is named in a structured event
    let parsed = parsed.unwrap();
    assert_eq!(parsed.packages.len(), 1);
    assert_eq!(parsed.packages[0].variant, Variant::Cuda12);
    assert!(parsed
        .newest(Engine::LlamaServer, Variant::Cuda12, "linux-x64")
        .is_some());
    let events = recorder.events.lock().unwrap();
    let skipped: Vec<(&str, &str)> = events
        .iter()
        .filter(|e| {
            e.get("message").map(String::as_str) == Some("engine_store.manifest.package_skipped")
        })
        .map(|e| (e["engine"].as_str(), e["variant"].as_str()))
        .collect();
    assert_eq!(
        skipped,
        [("llama-server", "sycl"), ("whisper-server", "cpu")]
    );
}

#[test]
fn test_a_known_package_still_malformed_refuses_the_manifest() {
    // GIVEN a manifest whose known package carries a field no package has
    let mut known = package("llama-server", "cuda-12", "cuda.zip");
    known["extra"] = serde_json::json!(true);
    let manifest = serde_json::json!({
        "format": 1, "channel": "stable", "sequence": 7, "packages": [known],
    });

    // WHEN it is parsed
    let parsed = Manifest::parse(&serde_json::to_vec(&manifest).unwrap());

    // THEN it is refused, as before
    assert!(
        matches!(parsed, Err(EngineStoreError::ManifestInvalid { .. })),
        "{parsed:?}"
    );
}

#[test]
fn test_the_committed_spec_lists_the_variants_the_runtime_publishes() {
    // GIVEN the document the runtime assembles and the committed spec
    use utoipa::OpenApi;
    let served: serde_json::Value =
        serde_json::from_str(&crate::api::openapi::ApiDoc::openapi().to_json().unwrap()).unwrap();
    let committed: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../clients/openapi.json"),
        )
        .unwrap(),
    )
    .unwrap();

    // WHEN the variant enum is read from each
    let variants =
        |doc: &serde_json::Value| doc["components"]["schemas"]["EngineVariant"]["enum"].clone();

    // THEN both list the same names, the new ones among them
    assert_eq!(variants(&served), variants(&committed));
    assert_eq!(
        variants(&served),
        serde_json::json!(["cuda-12", "cuda-13", "rocm", "vulkan", "cpu", "metal"])
    );
}
