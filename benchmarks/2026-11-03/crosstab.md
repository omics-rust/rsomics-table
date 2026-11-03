# Crosstab candidate: 2026-11-03

Apple M2, macOS arm64, Rust 1.91 release build; GNU datamash 1.9 built
locally from the official archive with `./configure --disable-nls` and
`make -j2`. Source archive SHA-256:
`f382ebda03650dd679161f758f9c0a6cc9293213438d4a77a8eda325aacb87d2`.

The fixture has 5,000,000 measurements, 500 sample labels, 40 feature labels,
and 59,600,021 bytes. For block `0..500` and row `i` in `0..10000`, sample is
`(i*37+block*13)%500`, feature `(i/500+block)%40`, and integer measurement
`1+(i*7+block)%100`. The keys recur nonadjacently. Sample names use `sNNN`,
features `fNN`; tab-separated header is `sample feature value`.

One paired warmup and five alternating-order measured pairs per operation.
Every invocation writes a complete matrix; all six native and six upstream
outputs per operation are byte-identical. `/usr/bin/time -l` records peak RSS.
The harness wall clock includes process startup and complete output handling.

| Operation | Native median seconds | Source median seconds | Native median RSS bytes | Source median RSS bytes |
|---|---:|---:|---:|---:|
| count | 0.488509 | 3.032328 | 7,290,880 | 624,656,384 |
| sum | 0.553847 | 3.012764 | 7,340,032 | 624,623,616 |

This qualifies the resource-use gate: all measured native RSS values are below
all measured upstream values, approximately 98.8% lower medians. Observed wall
ratios are 6.21x and 5.44x, but this was not an idle-host throughput campaign:
local test compilation overlapped part of the measurement window. Use the
resource decision, not a universal speed claim. The upstream command includes
sorting because that is necessary for correct global aggregation on this input;
this is an end-to-end workflow comparison, not an isolated reducer comparison.
No multithreaded crosstab claim is made.

Commands, all samples, complete source-body hashes, binaries, fixture and machine
are retained in `crosstab-samples.json`. Measured native executable SHA-256:
`e05cc3fb3dffad513ae3376cfe68541de1741816af7dd5b9d036c3a50161c9e6`.
The measured source is the candidate on top of `ac3788a`, with hidden command
visibility. No future release binary is represented as independently measured.

Original outputs, time logs, fixture, build logs and preserved executable:
`/Volumes/KIOXIA/Developments/tmp/table-crosstab-oracle-20261103.2rpinuLR/`.
`performance-corrected` is the acceptance packet; `performance` retains the
pre-review candidate and is not substituted for the corrected implementation.
