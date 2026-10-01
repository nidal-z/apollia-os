//! What a source file override may point at.
//!
//! The override exists so a model can load another weights file of itself
//! already on this machine, typically another quantisation kept in the same
//! folder. A projector or one part of a split file is not weights the engine
//! can load on its own, and the weights of another model would silently change
//! what the identifier names, so all three are refused.

use std::path::Path;

use super::scan::{has_extension, is_projector_name, is_safetensors, split_part};
use super::{IndexedModel, ModelFormat, ModelIndexError};
use crate::gguf_probe::{probe_file, SCREEN_BYTES};

/// Check that `candidate` may stand in for `model`'s weights.
pub(super) fn check_source_file(
    model: &IndexedModel,
    all: &[IndexedModel],
    candidate: &Path,
) -> Result<(), ModelIndexError> {
    let refuse_other = || ModelIndexError::SourceFromOtherModel {
        path: candidate.to_path_buf(),
    };
    if !candidate.is_file() {
        return Err(ModelIndexError::SourceNotFound {
            path: candidate.to_path_buf(),
        });
    }
    if is_projector_name(candidate) || all.iter().any(|m| m.annexes.iter().any(|a| a == candidate))
    {
        return Err(ModelIndexError::SourceIsProjector {
            path: candidate.to_path_buf(),
        });
    }
    if split_part(candidate).is_some() || all.iter().any(|m| m.parts.iter().any(|p| p == candidate))
    {
        return Err(ModelIndexError::SourceIsPart {
            path: candidate.to_path_buf(),
        });
    }

    match model.format {
        ModelFormat::Mlx => {
            if is_safetensors(candidate) && candidate.parent() == Some(model.path.as_path()) {
                Ok(())
            } else {
                Err(refuse_other())
            }
        }
        ModelFormat::Gguf => {
            if !has_extension(candidate, "gguf")
                || candidate == model.path
                || candidate.parent() != model.path.parent()
            {
                return Err(refuse_other());
            }
            if same_gguf_identity(&model.path, candidate) {
                Ok(())
            } else {
                Err(refuse_other())
            }
        }
    }
}

/// Two GGUF files are the same model when their headers declare the same
/// architecture and the same name. A header that cannot be read proves
/// nothing, so it does not match.
fn same_gguf_identity(a: &Path, b: &Path) -> bool {
    let (Ok(fa), Ok(fb)) = (probe_file(a, SCREEN_BYTES), probe_file(b, SCREEN_BYTES)) else {
        return false;
    };
    fa.architecture.is_some()
        && fa.name.is_some()
        && fa.architecture == fb.architecture
        && fa.name == fb.name
}
