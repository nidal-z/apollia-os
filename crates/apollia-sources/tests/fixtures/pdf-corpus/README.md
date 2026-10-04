# French PDF corpus

Twenty real French PDF documents, published by French public bodies on
<https://www.data.gouv.fr>, read by `crates/apollia-sources/tests/pdf_corpus.rs`.
They are contract documents and terms of use, budgets and price lists, and
methodological reports. None holds personal data: documents that name a
signatory, an elected official or a personal address were left out when the
corpus was chosen.

`witnesses.tsv` holds five witness phrases per document, one per line, as
`<file>\t<phrase>`. The phrases are lines of the text `pdftotext` (Poppler)
extracts, a reader independent of the one under test, picked by a fixed rule
that never looks at the output of the reader under test: a line of 25 to 90
characters, of at least five words, holding at least one accented letter, few
digits, no ligature and no list bullet, five of them spread evenly over the
document. The test passes
when at least nineteen of the twenty documents yield all five of their phrases,
whitespace runs collapsed, every accent intact.

Every file is unchanged from its source and is reused under the licence its
producer published it with on data.gouv.fr: the Licence Ouverte / Open Licence
of Etalab, version 2.0 (<https://www.etalab.gouv.fr/licence-ouverte-open-licence/>)
or version 1.0. Both allow reproduction and redistribution, commercial or not,
provided the source and the date of its last update are named; the source is
the URL in `sources.tsv`, whose path carries that date. `sources.tsv` names, for each
file, its kind, its producer, its licence, its source and its SHA-256.
