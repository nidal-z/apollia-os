//! PySttInterface: Python-facing proxy for agent speech-to-text operations.
//!
//! Exposes `ctx.stt.transcribe(path)` and `ctx.stt.status()` to Python agents.
//! When the STT backend is not configured, all transcription calls raise an error.

use std::sync::Arc;

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

use apollia_stt::SttBackend;

/// Python-facing interface for agent STT operations.
///
/// Each agent optionally receives a `PySttInterface` configured with an STT
/// backend. When the backend is `None` (STT not configured or model not loaded),
/// `transcribe()` raises a `RuntimeError` with an explicit message.
#[pyclass]
pub struct PySttInterface {
    backend: Option<Arc<dyn SttBackend>>,
    model_name: Option<String>,
    language: String,
}

impl PySttInterface {
    /// Creates a new `PySttInterface`.
    ///
    /// Pass `None` for `backend` when STT is not configured.
    /// `model_name` is surfaced via `status()`.
    /// `language` is the default language hint (e.g. `"auto"`, `"fr"`, `"en"`).
    pub fn new(
        backend: Option<Arc<dyn SttBackend>>,
        model_name: Option<String>,
        language: String,
    ) -> Self {
        Self {
            backend,
            model_name,
            language,
        }
    }
}

#[pymethods]
impl PySttInterface {
    /// Transcribes an audio file and returns the text.
    ///
    /// path: path to a WAV file (16-bit PCM or float32, mono or stereo).
    /// language: optional ISO 639-1 language code (e.g. "fr", "en"). `None` = context language.
    /// backend: backend override (reserved, ignored for now).
    ///
    /// Returns the transcribed text as a String.
    /// Raises RuntimeError if the STT backend is not configured or the file does not exist.
    #[pyo3(signature = (path, language = None, backend = None))]
    fn transcribe<'py>(
        &self,
        py: Python<'py>,
        path: String,
        language: Option<String>,
        backend: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let _ = backend; // reserved for future use

        let stt_backend = match self.backend.clone() {
            Some(b) => b,
            None => {
                return Err(PyRuntimeError::new_err(
                    "STT backend not available: no model loaded or STT not configured",
                ))
            }
        };

        let lang = language.or_else(|| {
            if self.language != "auto" {
                Some(self.language.clone())
            } else {
                None
            }
        });

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            if !std::path::Path::new(&path).exists() {
                return Err(PyRuntimeError::new_err(format!(
                    "audio file not found: {path}"
                )));
            }

            let result = tokio::task::spawn_blocking(move || {
                load_and_transcribe(stt_backend.as_ref(), &path, lang.as_deref())
            })
            .await
            .map_err(|e| PyRuntimeError::new_err(format!("spawn_blocking failed: {e}")))??;

            Ok(Python::with_gil(|py| {
                result.into_pyobject(py).unwrap().into_any().unbind()
            }))
        })
    }

    /// Returns the state of the STT engine.
    ///
    /// Returns a dict `{"enabled": bool, "model": str | None, "language": str}`.
    fn status<'py>(&self, _py: Python<'py>) -> PyResult<PyObject> {
        let enabled = self.backend.is_some();
        let model = self.model_name.clone();
        let language = self.language.clone();

        Python::with_gil(|py_inner| {
            let json_mod = py_inner
                .import("json")
                .map_err(|e| PyRuntimeError::new_err(format!("import json: {e}")))?;
            let json_str = serde_json::to_string(&serde_json::json!({
                "enabled": enabled,
                "model": model,
                "language": language,
            }))
            .map_err(|e| PyRuntimeError::new_err(format!("serialize: {e}")))?;
            let py_obj: PyObject = json_mod
                .call_method1("loads", (json_str,))
                .map_err(|e| PyRuntimeError::new_err(format!("json.loads: {e}")))?
                .unbind();
            Ok(py_obj)
        })
    }
}

/// Loads a WAV file and calls the STT backend.
///
/// Reads the file using `hound`, converts samples to mono f32, and calls
/// `backend.transcribe()`. Returns the full text transcript.
fn load_and_transcribe(
    backend: &dyn SttBackend,
    path: &str,
    language: Option<&str>,
) -> PyResult<String> {
    let mut reader = hound::WavReader::open(path)
        .map_err(|e| PyRuntimeError::new_err(format!("failed to open audio file '{path}': {e}")))?;

    let spec = reader.spec();
    let channels = spec.channels as usize;
    let sample_rate = spec.sample_rate;

    // Convert all samples to f32
    let raw_samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| PyRuntimeError::new_err(format!("failed to read float samples: {e}")))?,
        hound::SampleFormat::Int => {
            let max_val = (1i64 << (spec.bits_per_sample - 1)) as f32;
            match spec.bits_per_sample {
                16 => reader
                    .samples::<i16>()
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| {
                        PyRuntimeError::new_err(format!("failed to read i16 samples: {e}"))
                    })?
                    .into_iter()
                    .map(|s| s as f32 / max_val)
                    .collect(),
                32 => reader
                    .samples::<i32>()
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| {
                        PyRuntimeError::new_err(format!("failed to read i32 samples: {e}"))
                    })?
                    .into_iter()
                    .map(|s| s as f32 / max_val)
                    .collect(),
                _ => {
                    return Err(PyRuntimeError::new_err(format!(
                        "unsupported bit depth: {}",
                        spec.bits_per_sample
                    )))
                }
            }
        }
    };

    // Mix down to mono
    let mono: Vec<f32> = if channels == 1 {
        raw_samples
    } else {
        raw_samples
            .chunks_exact(channels)
            .map(|chunk| chunk.iter().sum::<f32>() / channels as f32)
            .collect()
    };

    let result = backend
        .transcribe(&mono, sample_rate, language)
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

    Ok(result.full_text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_stt::{SttError, TranscriptResult};

    struct MockSttBackend {
        response: String,
    }

    impl SttBackend for MockSttBackend {
        fn name(&self) -> &str {
            "mock"
        }

        fn transcribe(
            &self,
            _audio: &[f32],
            _sample_rate: u32,
            _language: Option<&str>,
        ) -> Result<TranscriptResult, SttError> {
            Ok(TranscriptResult {
                full_text: self.response.clone(),
                segments: vec![],
                language: None,
                audio_duration_ms: 0,
                processing_time_ms: 0,
            })
        }
    }

    // PySttInterface::new with no backend: backend is None
    #[test]
    fn test_status_disabled_when_no_backend() {
        // GIVEN no backend
        let iface = PySttInterface::new(None, None, "auto".to_string());
        assert!(iface.backend.is_none());
    }

    // PySttInterface::new with backend: fields are set correctly
    #[test]
    fn test_new_with_mock_backend() {
        // GIVEN a mock backend
        let backend: Arc<dyn SttBackend> = Arc::new(MockSttBackend {
            response: "hello".to_string(),
        });
        let iface = PySttInterface::new(
            Some(backend),
            Some("mock-model".to_string()),
            "fr".to_string(),
        );
        assert!(iface.backend.is_some());
        assert_eq!(iface.model_name.as_deref(), Some("mock-model"));
        assert_eq!(iface.language, "fr");
    }
}
