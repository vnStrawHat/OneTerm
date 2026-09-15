# Render `oneterm-vt`'s API reference and embedder's guide, and print the path
# to the guide's index page.
#
# The same `cargo doc` invocation CI runs, so a warning here is a CI failure
# there. It writes into `target/doc` like every other doc build; `ci-local`
# documents the crate with no features first and with `--all-features` last,
# because `vt-public-api.py --no-doc` reads whatever is left behind. Run this
# script after that check rather than between its two halves.
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")

$previous = $env:RUSTDOCFLAGS
$env:RUSTDOCFLAGS = "-D warnings"
try {
    cargo doc -p oneterm-vt --no-deps --all-features
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
finally {
    $env:RUSTDOCFLAGS = $previous
}

$index = Join-Path (Get-Location) "target/doc/oneterm_vt/guide/index.html"
Write-Host "file:///$($index -replace '\\', '/')"
