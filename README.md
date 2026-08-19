# rsomics-table

Strict, high-performance CSV and TSV workflows for bioinformatics.

Only completed operations appear in the command help:

```text
rsomics-table validate [OPTIONS] [TABLE]
rsomics-table select [OPTIONS] --fields <FIELDS> [TABLE]
rsomics-table filter [OPTIONS] --where <EXPRESSION> [TABLE]
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

`filter` parses one closed typed expression before streaming records. Field
references use `$1`, `$name`, or `${name with spaces}`. It supports finite
numbers, quoted UTF-8 strings, Booleans, `null`, arithmetic, comparisons,
`=~`, `!~`, literal-list `in`, `&&`, `||`, `!`, `len`, and `ulen`.

```bash
rsomics-table filter --where '$score >= 20 && $status == "case"' results.csv
rsomics-table filter --where '${sample name} =~ "^S[0-9]+$"' samples.csv
```

Fields that parse as finite numbers are numeric by default. Use
`--numeric-as-string` when exact numeric spelling is text. Unsupported
operators, type errors, invalid UTF-8 consumed by the expression, invalid
regexes, division by zero, and non-finite results fail nonzero.
