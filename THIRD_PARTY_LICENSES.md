# Third-party notices and source provenance

## csvtk

`rsomics-table` uses csvtk 0.37.0 at revision
`cc94b40d35cef9188d19f961718d9630479827c0` as the compatibility source for
CSV framing, field selection, filtering, sorting, and joins. Comparator and
natural-order behavior were informed by csvtk source.

Copyright © 2016-2019 Wei Shen, 2019 Oxford Nanopore Technologies.

## BEDTools

`rsomics-table groupby --consecutive` uses BEDTools 2.31.1 at revision
`705ccfdf2c9a77d71560c8adcece0663c2f5e18e` as a compatibility source for
operation spellings, output behavior, and fixtures.

Copyright © 2009-2019 Aaron Quinlan.

csvtk and BEDTools are licensed under the MIT License:

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

## GNU datamash

GNU datamash 1.9 is used only as an external black-box behavior and
documentation oracle. Its source is not copied, linked, or distributed with
`rsomics-table`. The release archive used by CI has SHA-256
`f382ebda03650dd679161f758f9c0a6cc9293213438d4a77a8eda325aacb87d2`.
GNU datamash is licensed GPL-3.0-or-later by Assaf Gordon, Tim Rice, Shawn
Wagner, and Erik Auerswald.

## Historical team-owned sources

These retired rsomics repositories are first-party implementation and test
assets, not third-party dependencies:

| Source | Revision | Retained material |
|---|---|---|
| `rsomics-csvio` | `0fccfb8cc2085a117ae88dc4b993c8b71b9c693b` | strict framing, writer behavior, field grammar, and fixtures |
| `rsomics-tsv-select` | `ba997aa55e050e4f40f25c84e657e5b0c2dd1dd0` | fixtures and benchmark recipe |
| `rsomics-tsv-filter` | `f694c99adab05a70800e93b3217e9a5507a68d63` | numeric cases and csvtk goldens |
| `rsomics-tsv-sort` | `1df47552324b55952ccd5e057f764833d24583e3` | comparator, natural ordering, quicksort, and differential fixtures |
| `rsomics-tsv-join` | `635603c8e2ff683707ef77827bc5520e482ad778` | fixtures and benchmark recipe |
| `rsomics-bed-groupby` | `30cf021d1c59785076912c59bade457ea4a4bc7a` | BEDTools formatting and edge cases |
| `rsomics-tsv-stats` | `108d43936350dafdbde2d1bd1cf6d4427941efd3` | aggregate operation set, numeric goldens, and tolerances |
