//! Optical character recognition: the text of a scanned page or a picture.
//!
//! [`Ocr`] is what a reader is handed to read a picture. It is available when
//! the engine is compiled in (the `ocr` feature) and its language data is on
//! this machine with the digests the registry pins
//! ([`apollia_llm::ocr_data::verified_languages`]); otherwise it is not, and a
//! reader marks the page instead of reading it. The engine is Tesseract,
//! linked statically: nothing is installed on the machine, nothing leaves it,
//! and nothing is written beside the documents.

use std::path::Path;

use image::GrayImage;

/// Why a picture that was handed to the engine gave no text.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum OcrError {
    /// The picture is larger than the engine addresses.
    #[error("the picture is too large to be recognised")]
    TooLarge,
    /// The engine failed on its language data or on the picture.
    #[error("the recognition engine failed: {0}")]
    Engine(String),
}

type Recognise = dyn Fn(&GrayImage) -> Result<String, OcrError> + Send + Sync;

/// The recognition a reader may call, or its absence.
pub struct Ocr {
    recognise: Option<Box<Recognise>>,
}

impl std::fmt::Debug for Ocr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ocr")
            .field("available", &self.is_available())
            .finish()
    }
}

impl Ocr {
    /// No recognition: every page that needs it is marked, none is read.
    #[must_use]
    pub fn unavailable() -> Self {
        Self { recognise: None }
    }

    /// Recognition by `recognise`, which is given the picture in grey levels
    /// and answers its text.
    #[must_use]
    pub fn with(
        recognise: impl Fn(&GrayImage) -> Result<String, OcrError> + Send + Sync + 'static,
    ) -> Self {
        Self {
            recognise: Some(Box::new(recognise)),
        }
    }

    /// Whether this build carries the engine.
    #[must_use]
    pub const fn compiled() -> bool {
        cfg!(feature = "ocr")
    }

    /// The embedded engine over the language data in `tessdata`: available
    /// when the engine is compiled in and at least one language the registry
    /// pins is there with its digest. The engine itself starts at the first
    /// picture.
    #[must_use]
    pub fn open(tessdata: &Path) -> Self {
        #[cfg(feature = "ocr")]
        {
            engine::open(tessdata)
        }
        #[cfg(not(feature = "ocr"))]
        {
            let _ = tessdata;
            Self::unavailable()
        }
    }

    /// Whether a picture handed to [`Self::read`] is read.
    #[must_use]
    pub fn is_available(&self) -> bool {
        self.recognise.is_some()
    }

    /// The text of `picture`; `None` when recognition is not available.
    pub(crate) fn read(&self, picture: &GrayImage) -> Option<Result<String, OcrError>> {
        self.recognise.as_ref().map(|recognise| recognise(picture))
    }
}

#[cfg(feature = "ocr")]
mod engine {
    use std::path::{Path, PathBuf};
    use std::sync::OnceLock;

    use apollia_llm::ocr_data::verified_languages;
    use apollia_llm::recommend::OcrCatalogue;
    use image::GrayImage;
    use tesseract_rs::TesseractAPI;

    use super::{Ocr, OcrError};

    /// Resolution given to the engine, in dots per inch: a picture embedded in
    /// a page or read from a file carries none it can rely on, and the engine
    /// guesses a far lower one when left to itself.
    const ASSUMED_DPI: i32 = 300;

    struct Engine {
        tessdata: PathBuf,
        languages: String,
        api: OnceLock<Result<TesseractAPI, String>>,
    }

    pub(super) fn open(tessdata: &Path) -> Ocr {
        let languages = match OcrCatalogue::embedded() {
            Ok(catalogue) => verified_languages(tessdata, &catalogue),
            Err(e) => {
                tracing::event!(tracing::Level::WARN, error = %e, "sources.ocr.registry_unreadable");
                Vec::new()
            }
        };
        // The engine takes its data path as text.
        if languages.is_empty() || tessdata.to_str().is_none() {
            return Ocr::unavailable();
        }
        let engine = Engine {
            tessdata: tessdata.to_path_buf(),
            languages: languages.join("+"),
            api: OnceLock::new(),
        };
        Ocr::with(move |picture| engine.recognise(picture))
    }

    impl Engine {
        fn start(&self) -> Result<TesseractAPI, String> {
            let api = TesseractAPI::new();
            api.init(&self.tessdata, &self.languages)
                .map_err(|e| e.to_string())?;
            tracing::event!(
                tracing::Level::INFO,
                languages = %self.languages,
                "sources.ocr.engine_started"
            );
            Ok(api)
        }

        fn recognise(&self, picture: &GrayImage) -> Result<String, OcrError> {
            let api = self
                .api
                .get_or_init(|| self.start())
                .as_ref()
                .map_err(|e| OcrError::Engine(e.clone()))?;
            let width = i32::try_from(picture.width()).map_err(|_| OcrError::TooLarge)?;
            let height = i32::try_from(picture.height()).map_err(|_| OcrError::TooLarge)?;
            let engine = |e: tesseract_rs::TesseractError| OcrError::Engine(e.to_string());
            api.set_image(picture.as_raw(), width, height, 1, width)
                .map_err(engine)?;
            api.set_source_resolution(ASSUMED_DPI).map_err(engine)?;
            api.get_utf8_text().map_err(engine)
        }
    }
}
