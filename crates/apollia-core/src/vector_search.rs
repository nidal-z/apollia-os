//! The arithmetic shared by every search that combines words and meaning.
//!
//! Three pieces, kept in one place so the index of a source and the memory of
//! an agent rank alike:
//!
//! - [`reciprocal_rank_fusion`] merges ranked lists (BM25, cosine) without
//!   comparing their scores, which live on unrelated scales;
//! - [`cosine`] and [`normalize`]: a stored vector has a Euclidean norm of one,
//!   so its cosine with a unit query is a dot product;
//! - [`encode_vector`] and [`decode_vector`]: the on-disk form of a vector, a
//!   BLOB of little-endian `f32`, four bytes per dimension.

use std::collections::HashMap;
use std::hash::Hash;

/// The constant `k` of reciprocal rank fusion: an item at rank `r` (counted
/// from one) in a list adds `1 / (k + r)` to its score.
///
/// Sixty is the value of the original paper (Cormack, Clarke and Buettcher,
/// 2009): large enough that the head of one list does not drown the other.
pub const RRF_K: f64 = 60.0;

/// Bytes per dimension of an encoded vector.
pub const BYTES_PER_DIMENSION: usize = 4;

/// Why a stored vector could not be read back.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum VectorDecodeError {
    /// The BLOB length is not a multiple of four bytes.
    #[error("a vector BLOB of {0} bytes is not a whole number of f32")]
    Truncated(usize),
}

/// Merges ranked lists by reciprocal rank.
///
/// Each list is ordered best first. An item scores the sum, over the lists
/// holding it, of `1 / (k + rank)` with `rank` counted from one; an item
/// repeated within a list counts at its first rank only. The result is sorted
/// by descending score; ties keep the order in which items were first met,
/// list by list, so the fusion is deterministic.
#[must_use]
pub fn reciprocal_rank_fusion<T>(rankings: &[Vec<T>], k: f64) -> Vec<(T, f64)>
where
    T: Clone + Eq + Hash,
{
    let mut scores: HashMap<T, (f64, usize)> = HashMap::new();
    let mut order = 0usize;
    for ranking in rankings {
        let mut seen: std::collections::HashSet<&T> = std::collections::HashSet::new();
        for (position, item) in ranking.iter().enumerate() {
            if !seen.insert(item) {
                continue;
            }
            let contribution = 1.0 / (k + (position as f64) + 1.0);
            let entry = scores.entry(item.clone()).or_insert_with(|| {
                order += 1;
                (0.0, order)
            });
            entry.0 += contribution;
        }
    }
    let mut fused: Vec<(T, f64, usize)> = scores
        .into_iter()
        .map(|(item, (score, first))| (item, score, first))
        .collect();
    fused.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.2.cmp(&b.2)));
    fused
        .into_iter()
        .map(|(item, score, _)| (item, score))
        .collect()
}

/// Scales `vector` to a Euclidean norm of one.
///
/// Returns `false`, leaving the vector unchanged, when it has no direction (a
/// zero norm, or a component that is not finite).
pub fn normalize(vector: &mut [f32]) -> bool {
    let norm = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
    if !norm.is_finite() || norm == 0.0 {
        return false;
    }
    for x in vector.iter_mut() {
        *x /= norm;
    }
    true
}

// TEST-ONLY: a search over stored vectors compares unit vectors with [`dot`],
// a third of the cost; this general form is the reference the tests hold
// `dot` to, and the comparison for a vector of any norm.
/// The cosine similarity of two vectors.
///
/// Vectors of different lengths, or a vector without direction, have no
/// similarity: the answer is `0.0`, which ranks them last rather than failing a
/// whole search over one bad row.
#[must_use]
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let (mut dot, mut norm_a, mut norm_b) = (0.0f32, 0.0f32, 0.0f32);
    for (x, y) in a.iter().zip(b) {
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }
    let denominator = norm_a.sqrt() * norm_b.sqrt();
    if denominator == 0.0 || !denominator.is_finite() {
        return 0.0;
    }
    dot / denominator
}

/// The dot product of two vectors, `0.0` when their lengths differ.
///
/// Equal to [`cosine`] when both have a norm of one, at a third of the cost:
/// the form a search over stored unit vectors uses.
#[must_use]
pub fn dot(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// The on-disk form of `vector`: little-endian `f32`, four bytes each.
#[must_use]
pub fn encode_vector(vector: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vector.len() * BYTES_PER_DIMENSION);
    for x in vector {
        bytes.extend_from_slice(&x.to_le_bytes());
    }
    bytes
}

/// Reads back a vector written by [`encode_vector`].
///
/// # Errors
/// [`VectorDecodeError::Truncated`] when the length is not a multiple of four.
pub fn decode_vector(bytes: &[u8]) -> Result<Vec<f32>, VectorDecodeError> {
    if !bytes.len().is_multiple_of(BYTES_PER_DIMENSION) {
        return Err(VectorDecodeError::Truncated(bytes.len()));
    }
    Ok(bytes
        .chunks_exact(BYTES_PER_DIMENSION)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect())
}

#[cfg(test)]
mod tests;
