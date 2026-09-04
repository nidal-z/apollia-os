Pristine workspace for the ORIA agent-loop eval suite.

Never edited by a run: prepare.sh copies this tree to ../run/ and the suite
works there. Editing a file here changes what the suite measures, and the
load check recomputes the digests that depend on it.
