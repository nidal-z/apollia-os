# Recognition fixtures

`scanned-fr.pdf`, `recu.png`, `recu.jpg` and `recu.tiff` were written for this
test suite and carry no third-party content: French sentences drawn to
pictures, as a scanner gives them. `scanned-fr.pdf` has three pages: one
picture each for the first two, and two pictures for the third, which
recognition therefore does not read.

The French language data the engine reads with is not kept here: it is over
the size a commit may add. The tests that need it are ignored by default and
read it from the folder `APOLLIA_TEST_TESSDATA` names, which must hold the
`fra.traineddata` the model registry pins.

They are read by `crates/apollia-sources/tests/ocr_reader.rs` and by the tests
of the indexer.
