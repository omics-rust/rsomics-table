# Release benchmarks

The release gate uses a deterministic 5,000,000-row streaming workload and a
60,000,000-row sort workload. The latter is larger than 2 GiB. Run it on a
quiet Linux x86_64 host with enough memory and a filesystem outside the macOS
boot disk.

```bash
benchmarks/build-oracles-linux-x86_64.sh \
  /external/rsomics-table-bench/oracles

rustc -O benchmarks/generate.rs -o /external/rsomics-table-bench/generate
/external/rsomics-table-bench/generate \
  /external/rsomics-table-bench/fixtures 5000000 60000000
gzip -n -k /external/rsomics-table-bench/fixtures/stream.csv

benchmarks/run-linux-x86_64.sh \
  /external/rsomics-table-bench/fixtures \
  /external/rsomics-table-bench/results \
  /external/rsomics-table-bench/bin/rsomics-table \
  /external/rsomics-table-bench/oracles/bin/csvtk \
  /external/rsomics-table-bench/oracles/bin/datamash \
  /external/rsomics-table-bench/oracles/bin/bedtools
```

The oracle binaries must be csvtk 0.37.0, GNU datamash 1.9, and BEDTools
2.31.1. The runner rejects other versions. Each comparable operation first
produces and byte-compares complete outputs. It then uses Hyperfine with three
paired warmups and ten runs. The paired runner randomizes which implementation
runs first while balancing first position across the complete sample, pins four
CPUs, and directs output to `/dev/null`. Raw Hyperfine JSON and the paired timing
table retain the execution order. GNU time records CPU and peak RSS separately.
The manifest captures revisions, binary and fixture hashes, commands, host
details, load, memory, and filesystem provenance.

The result directory must not already exist. Complete comparison outputs stay
under its `outputs` directory so the runner never deletes or overwrites an
evidence artifact.
