#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 6 ]]; then
  echo "usage: $0 FIXTURES RESULTS RSOMICS_TABLE CSVTK DATAMASH BEDTOOLS" >&2
  exit 2
fi

fixtures=$1
results=$2
rsomics=$3
csvtk=$4
datamash=$5
bedtools=$6
cpuset=${CPUSET:-0-3}
warmup=${WARMUP:-3}
runs=${RUNS:-10}
repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

[[ $(uname -s) == Linux && $(uname -m) == x86_64 ]]
[[ ! -e $results ]]
for command in hyperfine taskset sha256sum lscpu free; do
  command -v "$command" >/dev/null
done
for executable in "$rsomics" "$csvtk" "$datamash" "$bedtools" /usr/bin/time; do
  [[ -x $executable ]]
done

stream=$fixtures/stream.csv
stream_gzip=$fixtures/stream.csv.gz
sort_input=$fixtures/sort.csv
group_global=$fixtures/group-global.tsv
group_consecutive=$fixtures/group-consecutive.tsv
join_right=$fixtures/join-right.csv
for fixture in "$stream" "$stream_gzip" "$sort_input" "$group_global" "$group_consecutive" "$join_right"; do
  [[ -f $fixture ]]
done

[[ $($csvtk version) == "csvtk v0.37.0" ]]
"$datamash" --version | head -n 1 | grep -F 'datamash (GNU datamash) 1.9' >/dev/null
"$bedtools" --version | grep -F 'v2.31.1' >/dev/null

mkdir -p "$results/outputs"
export LC_ALL=C
export GOMAXPROCS=4

{
  date -u '+started_utc=%Y-%m-%dT%H:%M:%SZ'
  echo "revision=$(git -C "$repo" rev-parse HEAD)"
  echo "dirty_files=$(git -C "$repo" status --porcelain | wc -l)"
  echo "cpuset=$cpuset"
  echo "warmup=$warmup"
  echo "runs=$runs"
  uname -a
  lscpu
  free -b
  uptime
  df -hT "$fixtures" "$results"
  "$rsomics" --version
  "$csvtk" version
  "$datamash" --version | head -n 1
  "$bedtools" --version
  hyperfine --version
  /usr/bin/time --version | head -n 1
  sha256sum "$rsomics" "$csvtk" "$datamash" "$bedtools"
  find "$fixtures" -maxdepth 1 -type f -printf '%s %p\n' | sort
  sha256sum "$stream" "$stream_gzip" "$sort_input" "$group_global" "$group_consecutive" "$join_right"
} > "$results/manifest.txt"

shell_command() {
  printf -v command_text '%q ' "$@"
}

record_single() {
  local name=$1
  shell_command "${OURS_CMD[@]}"
  printf '%s\trsomics\t%s\n' "$name" "$command_text" >> "$results/commands.tsv"
  hyperfine --style basic --warmup "$warmup" --runs "$runs" \
    --export-json "$results/$name.json" \
    --command-name rsomics "$command_text > /dev/null 2>&1"
  /usr/bin/time -v -o "$results/$name.rsomics.time" \
    "${OURS_CMD[@]}" > /dev/null 2> "$results/$name.rsomics.stderr"
}

record_pair() {
  local name=$1
  local ours_output=$results/outputs/$name.rsomics
  local upstream_output=$results/outputs/$name.upstream
  "${OURS_CMD[@]}" > "$ours_output"
  "${UPSTREAM_CMD[@]}" > "$upstream_output"
  cmp "$ours_output" "$upstream_output"
  sha256sum "$ours_output" "$upstream_output" >> "$results/output-sha256.txt"
  shell_command "${OURS_CMD[@]}"
  local ours_text=$command_text
  shell_command "${UPSTREAM_CMD[@]}"
  local upstream_text=$command_text
  printf '%s\trsomics\t%s\n%s\tupstream\t%s\n' \
    "$name" "$ours_text" "$name" "$upstream_text" >> "$results/commands.tsv"
  hyperfine --style basic --warmup "$warmup" --runs "$runs" --randomize-order \
    --export-json "$results/$name.json" \
    --command-name rsomics "$ours_text > /dev/null 2>&1" \
    --command-name upstream "$upstream_text > /dev/null 2>&1"
  /usr/bin/time -v -o "$results/$name.rsomics.time" \
    "${OURS_CMD[@]}" > /dev/null 2> "$results/$name.rsomics.stderr"
  /usr/bin/time -v -o "$results/$name.upstream.time" \
    "${UPSTREAM_CMD[@]}" > /dev/null 2> "$results/$name.upstream.stderr"
}

prefix=(taskset -c "$cpuset")

OURS_CMD=("${prefix[@]}" "$rsomics" validate "$stream")
record_single validate_plain

OURS_CMD=("${prefix[@]}" "$rsomics" validate "$stream_gzip")
record_single validate_gzip

OURS_CMD=("${prefix[@]}" "$rsomics" select --fields id,value,label "$stream")
UPSTREAM_CMD=("${prefix[@]}" "$csvtk" cut --fields id,value,label "$stream")
record_pair select_plain

OURS_CMD=("${prefix[@]}" "$rsomics" select --fields id,value,label "$stream_gzip")
UPSTREAM_CMD=("${prefix[@]}" "$csvtk" cut --fields id,value,label "$stream_gzip")
record_pair select_gzip

OURS_CMD=("${prefix[@]}" "$rsomics" filter --where '$value >= 500000' "$stream")
UPSTREAM_CMD=("${prefix[@]}" "$csvtk" filter2 --filter '$value >= 500000' "$stream")
record_pair filter_plain

OURS_CMD=("${prefix[@]}" "$rsomics" sort --threads 4 --key value:n --key id:n "$sort_input")
UPSTREAM_CMD=("${prefix[@]}" "$csvtk" --num-cpus 4 sort --keys value:n,id:n "$sort_input")
record_pair sort_numeric

OURS_CMD=("${prefix[@]}" "$rsomics" join --on id "$stream" "$join_right")
UPSTREAM_CMD=("${prefix[@]}" "$csvtk" join --fields id "$stream" "$join_right")
record_pair join_inner

OURS_CMD=("${prefix[@]}" "$rsomics" groupby --tsv --no-header --no-output-header --group 2 \
  --aggregate 4:sum --aggregate 4:mean --aggregate 4:count "$group_global")
UPSTREAM_CMD=("${prefix[@]}" bash -c 'exec "$1" --sort --group 2 sum 4 mean 4 count 4 < "$2"' \
  _ "$datamash" "$group_global")
record_pair groupby_global_low

OURS_CMD=("${prefix[@]}" "$rsomics" groupby --tsv --no-header --no-output-header --group 3 \
  --aggregate 4:sum --aggregate 4:mean --aggregate 4:count "$group_global")
UPSTREAM_CMD=("${prefix[@]}" bash -c 'exec "$1" --sort --group 3 sum 4 mean 4 count 4 < "$2"' \
  _ "$datamash" "$group_global")
record_pair groupby_global_high

OURS_CMD=("${prefix[@]}" "$rsomics" groupby --tsv --no-header --no-output-header --consecutive \
  --group 2 --aggregate 4:sum --aggregate 4:mean --aggregate 4:count "$group_consecutive")
UPSTREAM_CMD=("${prefix[@]}" "$bedtools" groupby -g 2 -c 4,4,4 -o sum,mean,count -i "$group_consecutive")
record_pair groupby_consecutive

date -u '+finished_utc=%Y-%m-%dT%H:%M:%SZ' >> "$results/manifest.txt"
