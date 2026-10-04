# Office fixtures

These documents were written for this test suite, as minimal Office Open XML
archives, and carry no third-party content. They are read by
`crates/apollia-sources/tests/office_reader.rs`.

- `report.docx` holds, in its body: a paragraph split over two runs, an empty
  paragraph, an accented paragraph with a tab and entities, a table of three
  rows (a cell of two paragraphs, and an empty last row), a paragraph with a
  line break and a floating text box, and a paragraph inside a content
  control. A page header part holds text that is not part of the body.
- `deck.pptx` holds ten slides whose parts the archive lists in the order
  1, 10, 2 to 9; slide 4 draws no text. Slides 2 and 10 have speaker notes,
  in parts numbered otherwise than the slides, each with a slide number field.
- `inflated.docx` weighs about 80 KiB and its main part inflates to 80 MiB of
  spaces, over the decompressed-bytes cap.
