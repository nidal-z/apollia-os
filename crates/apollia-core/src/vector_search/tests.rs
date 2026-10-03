use super::*;

#[test]
fn test_fusion_ranks_an_item_found_by_both_lists_first() {
    // GIVEN a keyword ranking and a meaning ranking that agree on "b" only
    let keywords = vec!["a", "b", "c"];
    let meaning = vec!["d", "b", "e"];

    // WHEN they are fused with k = 60
    let fused = reciprocal_rank_fusion(&[keywords, meaning], RRF_K);

    // THEN "b" leads with 2 / 62, then the two heads tie and keep list order
    let order: Vec<&str> = fused.iter().map(|(item, _)| *item).collect();
    assert_eq!(order, ["b", "a", "d", "c", "e"]);
    assert!((fused[0].1 - 2.0 / 62.0).abs() < 1e-12);
    assert!((fused[1].1 - 1.0 / 61.0).abs() < 1e-12);
}

#[test]
fn test_fusion_counts_a_repeated_item_once_per_list() {
    // GIVEN a list that names the same item twice
    let ranking = vec!["x", "x", "y"];

    // WHEN it is fused alone
    let fused = reciprocal_rank_fusion(&[ranking], RRF_K);

    // THEN "x" counts at its first rank, and "y" keeps its own rank
    assert_eq!(fused.len(), 2);
    assert!((fused[0].1 - 1.0 / 61.0).abs() < 1e-12);
    assert!((fused[1].1 - 1.0 / 63.0).abs() < 1e-12);
}

#[test]
fn test_fusion_of_no_list_is_empty() {
    // GIVEN no ranking at all
    let rankings: Vec<Vec<u32>> = Vec::new();

    // WHEN they are fused
    let fused = reciprocal_rank_fusion(&rankings, RRF_K);

    // THEN nothing comes back
    assert!(fused.is_empty());
}

#[test]
fn test_cosine_of_parallel_orthogonal_and_mismatched_vectors() {
    // GIVEN two parallel vectors, two orthogonal ones, and two of unequal length
    let a = [1.0f32, 2.0, 2.0];
    let b = [2.0f32, 4.0, 4.0];
    let x = [1.0f32, 0.0];
    let y = [0.0f32, 3.0];

    // WHEN their cosines are taken
    let parallel = cosine(&a, &b);
    let orthogonal = cosine(&x, &y);
    let mismatched = cosine(&a, &x);

    // THEN they are one, zero, and zero
    assert!((parallel - 1.0).abs() < 1e-6);
    assert_eq!(orthogonal, 0.0);
    assert_eq!(mismatched, 0.0);
}

#[test]
fn test_normalize_gives_a_unit_vector_whose_dot_is_its_cosine() {
    // GIVEN two vectors of arbitrary norm
    let mut a = vec![3.0f32, 4.0];
    let mut b = vec![1.0f32, 1.0];
    let expected = cosine(&a, &b);

    // WHEN both are normalised
    let normalised = normalize(&mut a) && normalize(&mut b);

    // THEN each has a norm of one and their dot product equals their cosine
    assert!(normalised);
    assert!((dot(&a, &a) - 1.0).abs() < 1e-6);
    assert!((dot(&a, &b) - expected).abs() < 1e-6);
}

#[test]
fn test_normalize_refuses_a_zero_vector() {
    // GIVEN a vector without direction
    let mut zero = vec![0.0f32; 4];

    // WHEN it is normalised
    let normalised = normalize(&mut zero);

    // THEN it is refused and left as it was
    assert!(!normalised);
    assert_eq!(zero, [0.0; 4]);
}

#[test]
fn test_a_vector_survives_its_encoding() {
    // GIVEN a vector with negative, fractional and tiny components
    let vector = vec![-0.5f32, 0.25, 1e-20, 0.0, 1.0];

    // WHEN it is encoded and decoded
    let bytes = encode_vector(&vector);
    let back = decode_vector(&bytes).unwrap();

    // THEN it is four bytes per dimension and comes back bit for bit
    assert_eq!(bytes.len(), vector.len() * BYTES_PER_DIMENSION);
    assert_eq!(back, vector);
}

#[test]
fn test_a_truncated_blob_is_refused() {
    // GIVEN a BLOB of six bytes
    let bytes = [0u8; 6];

    // WHEN it is decoded
    let decoded = decode_vector(&bytes);

    // THEN it is refused with its length
    assert_eq!(decoded, Err(VectorDecodeError::Truncated(6)));
}
