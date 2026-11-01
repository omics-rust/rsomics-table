# Enabled Rust dependency notices

Native archives include the unchanged locked Rust dependencies selected by the
normal/build graphs for the four supported Linux/macOS targets. Original
copyright, license and notice bodies are copied under [LICENSES/crates](LICENSES/crates/).
[LICENSES/provenance.json](LICENSES/provenance.json) records each package's
declared license expression, original crate checksum, copied body identities
and per-target graph. Build/procedural-macro dependencies are included;
this is not an inventory of linked system libraries or a legal-clearance claim.

Unicode-ident's Unicode terms and the nested Unicode-table terms from
regex-syntax are retained alongside the crates' other license files. No
dependency license text is replaced with a generic template.

The separate upstream behavior sources and historical team-owned implementation
provenance remain in [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md). External
csvtk, BEDTools and GNU datamash oracle executables are not distributed.
