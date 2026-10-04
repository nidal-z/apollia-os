# Results corpus

These workbooks are copied unchanged from the test data of Apache POI, commit
`12c3688d130035f3dc2ca2a0f50d929456435a93`, directory `test-data/spreadsheet/`
(<https://github.com/apache/poi/tree/12c3688d130035f3dc2ca2a0f50d929456435a93/test-data/spreadsheet>).
They are read by `crates/apollia-sources/tests/results.rs`, which writes a
column of results into a copy of each and checks that every other part of the
archive is unchanged. Each one carries what a results column must not damage:

| File | Carries | SHA-256 |
|---|---|---|
| `WithConditionalFormatting.xlsx` | conditional formatting, a calculation chain | `ead4aad13d2b975f795b50e7c8b7da443fb1525a8f9fa7347a5f09041a3ed426` |
| `WithChart.xlsx` | a chart drawn from the sheet written | `51f27aabf7417f5aaa0542857b6bb56a92d4afbabdc5c63f084c06f92b67429d` |
| `ExcelPivotTableSample.xlsx` | a table and two pivot tables reading the sheet written | `f0ef1d61c9f3d8b27b18a876b8e24676314c6450ed64a725a7bfaa603fdd67e3` |
| `SimpleWithComments.xlsx` | comments on the sheet written | `d7d337b3436e863487d26439a57ef749950c599862325d7bce7e830acf1df0ba` |
| `70005-countifs.xlsx` | a workbook saved by LibreOffice 25.2 | `15cc9f6a1f08c631f3dd097e88c2f889ef7a8772c3625de58b594e7ddc932467` |

Every file weighs less than 500 KiB. The test also reads `issue221.xlsm` from
`../spreadsheets/` for a workbook with macros enabled.

They are distributed under the Apache License, Version 2.0
(<https://www.apache.org/licenses/LICENSE-2.0>), with this notice from the
project:

```text
Apache POI
Copyright 2003-2026 The Apache Software Foundation

This product includes software developed at
The Apache Software Foundation (https://www.apache.org/).
```
