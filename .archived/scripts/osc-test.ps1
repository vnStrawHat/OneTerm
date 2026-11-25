<#
.SYNOPSIS
    Interactive OSC escape-sequence tester for OneTerm (PowerShell edition).

.DESCRIPTION
    Run this INSIDE a OneTerm terminal (local PowerShell shell). Pick a number to
    fire exactly one OSC sequence so you can verify each feature in isolation.
    Query commands (OSC 10/11/12/4/52 with `?`) also read back and print the
    terminal's reply so you can confirm the response path works.

.EXAMPLE
    pwsh scripts/osc-test.ps1            # interactive menu

.EXAMPLE
    pwsh scripts/osc-test.ps1 11q        # run one test by id (see the menu)

.NOTES
    For SSH sessions to Linux or git-bash, use scripts/osc-test.sh instead.
#>
param([string]$Id)

$ESC = [char]27
$BEL = [char]7
$ST  = "$ESC\"

function Emit([string]$s) { [Console]::Out.Write($s); [Console]::Out.Flush() }

# Send a query sequence, then drain the terminal's reply (until ~1s of silence)
# and print it with the ESC byte made visible.
function Query([string]$seq) {
    Emit $seq
    Start-Sleep -Milliseconds 120
    $sb = New-Object System.Text.StringBuilder
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    try {
        while ($sw.ElapsedMilliseconds -lt 1000) {
            if ([Console]::KeyAvailable) {
                $k = [Console]::ReadKey($true)
                [void]$sb.Append($k.KeyChar)
                $sw.Restart()
            } else {
                Start-Sleep -Milliseconds 20
            }
        }
    } catch {
        Write-Host "  (cannot read reply: $($_.Exception.Message))" -ForegroundColor Yellow
        return
    }
    $reply = $sb.ToString()
    if ([string]::IsNullOrEmpty($reply)) {
        Write-Host "  (no reply within 1s)" -ForegroundColor Red
    } else {
        $visible = $reply.Replace([string]$ESC, '<ESC>').Replace([string]$BEL, '<BEL>')
        Write-Host "  reply: $visible"
    }
}

function Pause-Key { Write-Host "(enter to continue)" -ForegroundColor DarkGray; [void](Read-Host) }

# ── Individual tests ─────────────────────────────────────────────────────────
function Invoke-Osc([string]$id) {
    switch ($id) {
        '0'    { Emit "$ESC]0;OneTerm OSC0 title$BEL"; "OSC 0  -> title+icon set to 'OneTerm OSC0 title'" }
        '2'    { Emit "$ESC]2;OneTerm OSC2 title$BEL"; "OSC 2  -> window title set to 'OneTerm OSC2 title'" }

        '4'    { Emit "$ESC]4;1;rgb:00ff/0000/0000$BEL"; "OSC 4  -> palette index 1 (red) set to #ff0000. Print a red char (e.g. `e[31mX) to see it." }
        '4b'   { Emit "$ESC]4;1;#00ff00$BEL"; "OSC 4  -> palette index 1 set to #00ff00 (hex form)." }
        '4q'   { "OSC 4  query index 1:"; Query "$ESC]4;1;?$BEL" }
        '104'  { Emit "$ESC]104;1$BEL"; "OSC 104 -> palette index 1 reset to theme default." }
        '104a' { Emit "$ESC]104$BEL"; "OSC 104 -> ALL palette colors reset to theme default." }

        '7'    { Emit "$ESC]7;file://localhost/tmp$BEL"; "OSC 7  -> cwd set to /tmp (check breadcrumb)." }
        '8'    { Emit "$ESC]8;;https://github.com${ST}OneTerm repo link$ESC]8;;$ST"; "  <- Ctrl+click the link above (OSC 8)." }

        '9'    { Emit "$ESC]9;Hello from OneTerm (OSC 9)$BEL"; "OSC 9  -> desktop notification (expect a toast)." }

        '94'   {
            "OSC 9;4 -> animating progress 0->100 (normal), then remove..."
            foreach ($p in 0,20,40,60,80,100) { Emit "$ESC]9;4;1;$p$BEL"; Start-Sleep -Milliseconds 400 }
            Start-Sleep -Milliseconds 500; Emit "$ESC]9;4;0$BEL"; "  done (progress removed)."
        }
        '94e'  { Emit "$ESC]9;4;2;66$BEL"; "OSC 9;4 -> error state @ 66% (expect danger-colored bar)." }
        '94i'  { Emit "$ESC]9;4;3$BEL"; "OSC 9;4 -> indeterminate (expect full-width bar)." }
        '94p'  { Emit "$ESC]9;4;4;50$BEL"; "OSC 9;4 -> paused @ 50% (expect warning-colored bar)." }
        '94x'  { Emit "$ESC]9;4;0$BEL"; "OSC 9;4 -> remove progress." }

        '10'   { Emit "$ESC]10;rgb:ffff/8000/0000$BEL"; "OSC 10 -> default foreground set to orange." }
        '11'   { Emit "$ESC]11;rgb:0000/2000/4000$BEL"; "OSC 11 -> default background set to dark blue." }
        '12'   { Emit "$ESC]12;rgb:00ff/ff00/0000$BEL"; "OSC 12 -> cursor color set to yellow." }
        '10q'  { "OSC 10 query (foreground):"; Query "$ESC]10;?$BEL" }
        '11q'  { "OSC 11 query (background):"; Query "$ESC]11;?$BEL" }
        '12q'  { "OSC 12 query (cursor):";     Query "$ESC]12;?$BEL" }
        '110'  { Emit "$ESC]110$BEL"; "OSC 110 -> foreground reset to theme default." }
        '111'  { Emit "$ESC]111$BEL"; "OSC 111 -> background reset to theme default." }
        '112'  { Emit "$ESC]112$BEL"; "OSC 112 -> cursor color reset to theme default." }

        '52'   {
            $b64 = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes("OneTerm OSC52 clipboard"))
            Emit "$ESC]52;c;$b64$BEL"; "OSC 52 -> wrote 'OneTerm OSC52 clipboard' to the clipboard (paste to verify)."
        }
        '52q'  { "OSC 52 query (clipboard read):"; Query "$ESC]52;c;?$BEL" }

        '133'  {
            "OSC 133 -> emitting a full prompt cycle (A/B/C/D;0)..."
            Emit "$ESC]133;A${BEL}fake`$ "
            Emit "$ESC]133;B${BEL}echo hi`n"
            Emit "$ESC]133;C${BEL}hi`n"
            Emit "$ESC]133;D;0$BEL"; "  done (prompt_count should increment)."
        }
        default { Write-Host "Unknown id: $id" -ForegroundColor Red }
    }
}

function Show-Menu {
    Write-Host @'

  -- OneTerm OSC tester -----------------------------------------------
   Title      :  0  OSC 0 title      2   OSC 2 title
   Palette    :  4  set idx1 (rgb)   4b  set idx1 (#hex)   4q  query idx1
                104 reset idx1      104a reset ALL
   CWD/Link   :  7  OSC 7 cwd         8   OSC 8 hyperlink
   Notify     :  9  OSC 9 notification
   Progress   : 94  animate 0->100   94e error   94i indeterminate
                94p paused           94x remove
   FG/BG/Cur  : 10  set fg   11 set bg   12 set cursor
                10q query fg   11q query bg   12q query cursor
                110 reset fg   111 reset bg   112 reset cursor
   Clipboard  : 52  OSC 52 set        52q OSC 52 query
   Shell int. : 133 OSC 133 A/B/C/D
   ---------------------------------------------------------------------
   q  quit
'@
}

# ── One-shot mode: `osc-test.ps1 <id>` ───────────────────────────────────────
if ($PSBoundParameters.ContainsKey('Id') -and $Id) {
    Invoke-Osc $Id
    return
}

# ── Interactive loop ─────────────────────────────────────────────────────────
while ($true) {
    Show-Menu
    $choice = Read-Host '  >'
    switch -Regex ($choice) {
        '^(q|quit|exit)$' { Write-Host 'bye.'; return }
        '^\s*$'           { }
        default           { Invoke-Osc $choice.Trim(); Pause-Key }
    }
}
