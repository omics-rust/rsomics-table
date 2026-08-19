#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 ROOT" >&2
  exit 2
fi

root=$1
csvtk_revision=cc94b40d35cef9188d19f961718d9630479827c0
csvtk_tag=v0.37.0
csvtk_sha256=90068a24f055076d65f54b18fa796b5322ffe728687d972038bb2ffa2ca07be8
bedtools_revision=705ccfdf2c9a77d71560c8adcece0663c2f5e18e
bedtools_tag=v2.31.1
bedtools_sha256=79a1ba318d309f4e74bfa74258b73ef578dccb1045e270998d7fe9da9f43a50e
datamash_sha256=f382ebda03650dd679161f758f9c0a6cc9293213438d4a77a8eda325aacb87d2
go_sha256=9e9b755d63b36acf30c12a9a3fc379243714c1c6d3dd72861da637f336ebb35b

[[ $(uname -s) == Linux && $(uname -m) == x86_64 ]]
[[ ! -e $root ]]
for command in curl make tar sha256sum gcc g++ ar ranlib; do
  command -v "$command" >/dev/null
done
for header in /usr/include/bzlib.h /usr/include/lzma.h /usr/include/zlib.h; do
  [[ -f $header ]]
done

mkdir -p "$root/bin" "$root/cache" "$root/downloads" "$root/source" "$root/toolchain"

go_archive=$root/downloads/go1.25.5.linux-amd64.tar.gz
curl -fsSLo "$go_archive" https://dl.google.com/go/go1.25.5.linux-amd64.tar.gz
echo "$go_sha256  $go_archive" | sha256sum --check --status
tar -xzf "$go_archive" -C "$root/toolchain"
go=$root/toolchain/go/bin/go
[[ $($go version) == 'go version go1.25.5 linux/amd64' ]]
export GOCACHE=$root/cache/go-build
export GOMODCACHE=$root/cache/go-mod
export GOPATH=$root/cache/gopath
export GOPROXY=https://goproxy.cn,direct
export GOSUMDB=sum.golang.google.cn

csvtk_archive=$root/downloads/csvtk-$csvtk_tag.tar.gz
curl -fsSLo "$csvtk_archive" "https://codeload.github.com/shenwei356/csvtk/tar.gz/refs/tags/$csvtk_tag"
echo "$csvtk_sha256  $csvtk_archive" | sha256sum --check --status
tar -xzf "$csvtk_archive" -C "$root/source"
csvtk_source=$root/source/csvtk-0.37.0
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

bedtools_archive=$root/downloads/bedtools-$bedtools_tag.tar.gz
curl -fsSLo "$bedtools_archive" "https://codeload.github.com/arq5x/bedtools2/tar.gz/refs/tags/$bedtools_tag"
echo "$bedtools_sha256  $bedtools_archive" | sha256sum --check --status
tar -xzf "$bedtools_archive" -C "$root/source"
bedtools_source=$root/source/bedtools2-2.31.1
make -C "$bedtools_source" -j2
install -m 0755 "$bedtools_source/bin/bedtools" "$root/bin/bedtools"
"$root/bin/bedtools" --version | grep -F 'v2.31.1' >/dev/null

{
  date -u '+built_utc=%Y-%m-%dT%H:%M:%SZ'
  "$go" version
  echo "GOPROXY=$GOPROXY"
  echo "GOSUMDB=$GOSUMDB"
  "$root/bin/csvtk" version
  "$root/bin/datamash" --version | head -n 1
  "$root/bin/bedtools" --version
  echo "csvtk_revision=$csvtk_revision"
  echo "csvtk_archive_sha256=$csvtk_sha256"
  echo "bedtools_revision=$bedtools_revision"
  echo "bedtools_archive_sha256=$bedtools_sha256"
  echo "datamash_archive_sha256=$datamash_sha256"
  echo "go_archive_sha256=$go_sha256"
  sha256sum "$root/bin/csvtk" "$root/bin/datamash" "$root/bin/bedtools"
} > "$root/manifest.txt"
