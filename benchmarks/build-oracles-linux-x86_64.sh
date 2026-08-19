#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 ROOT" >&2
  exit 2
fi

root=$1
csvtk_revision=cc94b40d35cef9188d19f961718d9630479827c0
csvtk_tag=v0.37.0
bedtools_revision=705ccfdf2c9a77d71560c8adcece0663c2f5e18e
bedtools_tag=v2.31.1
datamash_sha256=f382ebda03650dd679161f758f9c0a6cc9293213438d4a77a8eda325aacb87d2
go_sha256=9e9b755d63b36acf30c12a9a3fc379243714c1c6d3dd72861da637f336ebb35b

[[ $(uname -s) == Linux && $(uname -m) == x86_64 ]]
[[ ! -e $root ]]
for command in curl git make tar sha256sum gcc g++ ar ranlib; do
  command -v "$command" >/dev/null
done
for header in /usr/include/bzlib.h /usr/include/lzma.h /usr/include/zlib.h; do
  [[ -f $header ]]
done

mkdir -p "$root/bin" "$root/cache" "$root/downloads" "$root/source" "$root/toolchain"

go_archive=$root/downloads/go1.25.5.linux-amd64.tar.gz
curl -fsSLo "$go_archive" https://go.dev/dl/go1.25.5.linux-amd64.tar.gz
echo "$go_sha256  $go_archive" | sha256sum --check --status
tar -xzf "$go_archive" -C "$root/toolchain"
go=$root/toolchain/go/bin/go
[[ $($go version) == 'go version go1.25.5 linux/amd64' ]]
export GOCACHE=$root/cache/go-build
export GOMODCACHE=$root/cache/go-mod
export GOPATH=$root/cache/gopath

csvtk_source=$root/source/csvtk
git init -q "$csvtk_source"
git -C "$csvtk_source" remote add origin https://github.com/shenwei356/csvtk.git
git -C "$csvtk_source" fetch -q --depth=1 origin "refs/tags/$csvtk_tag:refs/tags/$csvtk_tag"
git -C "$csvtk_source" checkout -q --detach "$csvtk_tag"
[[ $(git -C "$csvtk_source" rev-parse HEAD) == "$csvtk_revision" ]]
(
  cd "$csvtk_source"
  "$go" build -trimpath -o "$root/bin/csvtk" ./csvtk
)
[[ $($root/bin/csvtk version) == 'csvtk v0.37.0' ]]

datamash_archive=$root/downloads/datamash-1.9.tar.gz
curl -fsSLo "$datamash_archive" https://ftp.gnu.org/gnu/datamash/datamash-1.9.tar.gz
echo "$datamash_sha256  $datamash_archive" | sha256sum --check --status
tar -xzf "$datamash_archive" -C "$root/source"
(
  cd "$root/source/datamash-1.9"
  ./configure --prefix="$root/datamash"
  make -j2
  make install
)
install -m 0755 "$root/datamash/bin/datamash" "$root/bin/datamash"
"$root/bin/datamash" --version | head -n 1 | grep -F 'datamash (GNU datamash) 1.9' >/dev/null

bedtools_source=$root/source/bedtools2
git init -q "$bedtools_source"
git -C "$bedtools_source" remote add origin https://github.com/arq5x/bedtools2.git
git -C "$bedtools_source" fetch -q --depth=1 origin "refs/tags/$bedtools_tag:refs/tags/$bedtools_tag"
git -C "$bedtools_source" checkout -q --detach "$bedtools_tag"
[[ $(git -C "$bedtools_source" rev-parse HEAD) == "$bedtools_revision" ]]
make -C "$bedtools_source" -j2
install -m 0755 "$bedtools_source/bin/bedtools" "$root/bin/bedtools"
"$root/bin/bedtools" --version | grep -F 'v2.31.1' >/dev/null

{
  date -u '+built_utc=%Y-%m-%dT%H:%M:%SZ'
  "$go" version
  "$root/bin/csvtk" version
  "$root/bin/datamash" --version | head -n 1
  "$root/bin/bedtools" --version
  echo "csvtk_revision=$csvtk_revision"
  echo "bedtools_revision=$bedtools_revision"
  echo "datamash_archive_sha256=$datamash_sha256"
  echo "go_archive_sha256=$go_sha256"
  sha256sum "$root/bin/csvtk" "$root/bin/datamash" "$root/bin/bedtools"
} > "$root/manifest.txt"
