# rsomics-table

Strict, high-performance CSV and TSV workflows for bioinformatics.

Only completed operations appear in the command help. The current command is:

```text
rsomics-table validate [OPTIONS] [TABLE]
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
