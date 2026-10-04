use super::*;

/// A paragraph of `words` distinct words, ten bytes each with the space
/// after it (two and a half tokens a word, by the estimate).
fn paragraph(tag: &str, words: usize) -> String {
    (0..words)
        .map(|i| format!("{tag}{i:05}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn test_the_estimate_is_one_token_per_four_bytes_rounded_up() {
    // GIVEN texts of zero, one, four, five and accented bytes
    // WHEN their tokens are estimated
    // THEN each is its byte length over four, rounded up
    assert_eq!(token_estimate(""), 0);
    assert_eq!(token_estimate("a"), 1);
    assert_eq!(token_estimate("abcd"), 1);
    assert_eq!(token_estimate("abcde"), 2);
    assert_eq!(token_estimate("\u{e9}\u{e9}\u{e9}"), 2);
}

#[test]
fn test_a_long_text_is_cut_at_paragraphs_into_chunks_of_about_400_tokens() {
    // GIVEN a text of twenty paragraphs of about 125 tokens each
    let text = (0..20)
        .map(|p| paragraph(&format!("p{p:02}w"), 50))
        .collect::<Vec<_>>()
        .join("\n\n");

    // WHEN it is chunked
    let chunks = chunk_document(&[Section::Text { text }]);

    // THEN every chunk weighs at most 400 tokens, every one but the last at
    //      least 250, and a chunk ends only where a paragraph ends
    assert!(chunks.len() >= 5, "{}", chunks.len());
    for chunk in &chunks {
        let tokens = token_estimate(&chunk.text);
        assert!(tokens <= TARGET_TOKENS, "{tokens}");
    }
    for chunk in &chunks[..chunks.len() - 1] {
        let tokens = token_estimate(&chunk.text);
        assert!(tokens >= 250, "{tokens}");
        let last_line = chunk.text.lines().last().unwrap();
        assert!(
            last_line.ends_with("w00049"),
            "cut inside a paragraph: {last_line}"
        );
    }
}

#[test]
fn test_each_chunk_repeats_about_50_tokens_of_the_one_before() {
    // GIVEN a text long enough for several chunks
    let text = (0..12)
        .map(|p| paragraph(&format!("q{p:02}w"), 50))
        .collect::<Vec<_>>()
        .join("\n\n");

    // WHEN it is chunked
    let chunks = chunk_document(&[Section::Text { text }]);

    // THEN each chunk after the first opens with the closing words of the
    //      previous one, worth between 40 and 50 tokens
    assert!(chunks.len() >= 2);
    for pair in chunks.windows(2) {
        let overlap = pair[1].text.lines().next().unwrap();
        assert!(pair[0].text.ends_with(overlap), "{overlap}");
        let tokens = token_estimate(overlap);
        assert!((40..=OVERLAP_TOKENS).contains(&tokens), "{tokens}");
    }
}

#[test]
fn test_lines_of_a_text_file_are_located() {
    // GIVEN a text whose second paragraph starts on line 4
    let text = "first line\nsecond line\n\nthird paragraph\nends here\n".to_owned();

    // WHEN it is chunked
    let chunks = chunk_document(&[Section::Text { text }]);

    // THEN one chunk covers lines 1 to 5
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].location, Location::Lines { first: 1, last: 5 });
}

#[test]
fn test_a_chunk_never_spans_two_pages_or_two_slides() {
    // GIVEN two short pages and two short slides
    let sections = [
        Section::Page {
            page: 1,
            text: "The contract is signed.".to_owned(),
        },
        Section::Page {
            page: 2,
            text: "The payment is due.".to_owned(),
        },
        Section::Slide {
            slide: 7,
            text: "Goals of the quarter".to_owned(),
        },
        Section::Slide {
            slide: 8,
            text: "Results".to_owned(),
        },
    ];

    // WHEN they are chunked
    let chunks = chunk_document(&sections);

    // THEN each keeps its own chunk and location, with no overlap across them
    let located: Vec<(&str, &Location)> = chunks
        .iter()
        .map(|c| (c.text.as_str(), &c.location))
        .collect();
    assert_eq!(
        located,
        [
            ("The contract is signed.", &Location::Page { page: 1 }),
            ("The payment is due.", &Location::Page { page: 2 }),
            ("Goals of the quarter", &Location::Slide { slide: 7 }),
            ("Results", &Location::Slide { slide: 8 }),
        ]
    );
}

#[test]
fn test_rows_of_a_sheet_are_cut_between_rows_and_located() {
    // GIVEN a sheet of 300 rows of about ten tokens each, row 2 left empty
    let rows: Vec<(u32, String)> = (1..=300)
        .map(|r| {
            let text = if r == 2 {
                String::new()
            } else {
                format!("customer {r:04} | Lyon | 1200 euros | paid")
            };
            (r, text)
        })
        .collect();

    // WHEN the sheet is chunked
    let chunks = chunk_document(&[Section::Sheet {
        name: "Sales".to_owned(),
        rows,
    }]);

    // THEN the rows are cut between rows, every row is in exactly one chunk's
    //      own range, and the ranges follow each other
    assert!(chunks.len() > 1);
    let mut next = 1u32;
    for chunk in &chunks {
        let Location::Rows { sheet, first, last } = &chunk.location else {
            panic!("not rows: {:?}", chunk.location);
        };
        assert_eq!(sheet, "Sales");
        assert!(
            *first == next || (next == 2 && *first == 3),
            "{first} after {next}"
        );
        assert!(chunk.text.trim_end().ends_with("paid"));
        next = last + 1;
    }
    assert_eq!(next, 301);
}

#[test]
fn test_a_paragraph_longer_than_a_chunk_is_cut_between_words() {
    // GIVEN one paragraph of about 1 000 tokens on a single line
    let text = paragraph("long", 500);

    // WHEN it is chunked
    let chunks = chunk_document(&[Section::Text { text }]);

    // THEN it spans several chunks of at most 400 tokens, none cutting a word
    assert!(chunks.len() >= 3, "{}", chunks.len());
    for chunk in &chunks {
        assert!(token_estimate(&chunk.text) <= TARGET_TOKENS);
        assert!(
            chunk.text.split_whitespace().all(|w| w.len() == 9),
            "a word was cut"
        );
    }
}

#[test]
fn test_a_location_is_stored_as_tagged_json() {
    // GIVEN a range of rows
    let location = Location::Rows {
        sheet: "Sales".to_owned(),
        first: 3,
        last: 9,
    };

    // WHEN it is written and read back
    let json = serde_json::to_string(&location).unwrap();
    let back: Location = serde_json::from_str(&json).unwrap();

    // THEN its kind is named and nothing is lost
    assert_eq!(
        json,
        r#"{"kind":"rows","sheet":"Sales","first":3,"last":9}"#
    );
    assert_eq!(back, location);
}
