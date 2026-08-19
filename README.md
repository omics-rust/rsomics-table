# rsomics-table

Strict, high-performance CSV and TSV workflows for bioinformatics.

Only completed operations appear in the command help:

```text
rsomics-table validate [OPTIONS] [TABLE]
rsomics-table select [OPTIONS] --fields <FIELDS> [TABLE]
```

`validate` checks CSV/TSV quoting, row width, headers, optional UTF-8, and
plain or gzip stream integrity. Input has a header by default; use
`--no-header` for headerless tables and `--tsv` for tab-delimited input.

```bash
rsomics-table validate --tsv samples.tsv
rsomics-table --json validate results.csv.gz
```

Malformed input fails nonzero with record, physical-line, and byte-offset
context where framing remains available. JSON reports use the shared rsomics
machine-output envelope.

`select` projects fields by one-based index, inclusive or open range, exact
header name, `${...}` name, exclusion, or an optional fuzzy pattern. Selection
order and repeated fields are preserved, while missing fields and duplicate
headers fail.

```bash
rsomics-table select --fields 'sample,score' results.csv
rsomics-table select --tsv --fields '3-,1' samples.tsv
rsomics-table --json select --fields name --output names.csv results.csv.gz
```

Named output is transactional and `.gz` selects gzip compression. Use
`--gzip` when compressed output is written to standard output.
