# rsomics-table

Strict, high-performance CSV and TSV workflows for bioinformatics.

Only completed operations appear in the command help:

```text
rsomics-table validate [OPTIONS] [TABLE]
rsomics-table select [OPTIONS] --fields <FIELDS> [TABLE]
rsomics-table filter [OPTIONS] --where <EXPRESSION> [TABLE]
rsomics-table sort [OPTIONS] [TABLE]
rsomics-table join [OPTIONS] <LEFT> <RIGHT>
rsomics-table groupby [OPTIONS] --aggregate <SPEC> [TABLE]
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

`sort` accepts repeated `--key FIELD[:TYPE]` values. The default type is byte
lexical order; `n` selects numeric order, `N` natural order, and `r` reverses
one key. Field ranges and comma lists use the same grammar as `select`.

```bash
rsomics-table sort --key score:nr --key sample results.csv
rsomics-table sort --tsv --key chrom:N --threads 4 variants.tsv
```

Numeric spellings with commas and non-numeric values follow csvtk 0.37.0 sort
semantics. Equal-key output uses its deterministic permutation at every thread
count. Date and custom-level keys are rejected rather than silently treated as
text.

`join` supports inner, left, and full joins over one or more keys. Use `--on`
when both tables share key names, or pair `--left-on` with `--right-on`.
Duplicate keys produce the complete Cartesian product in input order.

```bash
rsomics-table join --on sample metadata.csv counts.csv
rsomics-table join --left-on id --right-on sample_id --kind full --fill NA left.csv right.csv
```

Composite keys use collision-free byte framing. Full joins append unmatched
right rows in their original order, and colliding right-side headers receive a
checked `_right` suffix. `--null-never-matches` prevents any key containing an
empty field from matching.

`groupby` combines equal composite keys globally and writes keys in byte-sorted
order. Aggregates use `FIELD:OPERATION[=ALIAS]`; repeat `--aggregate` to produce
multiple values. Without `--group`, the complete input is one group.

```bash
rsomics-table groupby --group condition --aggregate count:sum=total counts.csv
rsomics-table groupby --tsv --group sample,feature --aggregate value:mean matrix.tsv
```

`--consecutive` keeps one active run for already grouped input and fails if a
key reappears later. Numeric cells fail loudly by default;
`--ignore-non-numeric` skips them and reports the number skipped. Numeric,
order-statistic, and text operations are listed in `groupby --help`.

Release compatibility is pinned to csvtk 0.37.0, GNU datamash 1.9, and
BEDTools 2.31.1. Their operation-specific roles, revisions, licenses, and the
historical team-owned source record are in
[THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md).
