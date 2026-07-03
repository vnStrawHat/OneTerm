$dir = $PSScriptRoot
$out = Join-Path (Split-Path $dir -Parent) 'sftp-follow-terminal-cwd.md'
$parts = @(
    '01-overview.md',
    '02-current-state.md',
    '03-high-level-design.md',
    '04-low-level-design.md',
    '05-edge-cases-roadmap.md'
)
$utf8 = New-Object System.Text.UTF8Encoding($false)  # no BOM
$sb = New-Object System.Text.StringBuilder
foreach ($p in $parts) {
    $content = [System.IO.File]::ReadAllText((Join-Path $dir $p), $utf8)
    [void]$sb.Append($content)
    [void]$sb.Append("`n")
}
[System.IO.File]::WriteAllText($out, $sb.ToString(), $utf8)
Write-Output ("merged -> " + $out + "  (" + (Get-Item $out).Length + " bytes)")
