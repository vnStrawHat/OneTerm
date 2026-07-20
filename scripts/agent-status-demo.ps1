<#
.SYNOPSIS
    Simulates coding-agent activity in OneTerm by emitting OSC 9;7 events.

.DESCRIPTION
    Run this script inside a OneTerm terminal. The default scenario exercises
    the folded Agent Panel card: session, model/context, lifecycle, tool, file,
    and approval data. Use Multi to create multiple agent cards in one Space.
    Run the script in terminals from different Tabs/Spaces to verify grouping.

.PARAMETER Scenario
    One of Demo, Working, Blocked, Idle, Error, Done, or Multi.

.PARAMETER Agent
    Lowercase ASCII agent identifier used by all single-agent scenarios.

.PARAMETER DelayMs
    Delay after each event. Use 0 for an instant run.

.EXAMPLE
    pwsh scripts/agent-status-demo.ps1

.EXAMPLE
    pwsh scripts/agent-status-demo.ps1 -Scenario Blocked -Agent codex -DelayMs 1000

.EXAMPLE
    pwsh scripts/agent-status-demo.ps1 -Scenario Multi
#>
[CmdletBinding()]
param(
    [ValidateSet('Demo', 'Working', 'Blocked', 'Idle', 'Error', 'Done', 'Multi')]
    [string]$Scenario = 'Demo',

    [ValidatePattern('^[a-z][a-z0-9_-]*$')]
    [string]$Agent = 'pi',

    [ValidateRange(0, [int]::MaxValue)]
    [int]$DelayMs = 600
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$ESC = [char]27
$BEL = [char]7
$script:Sequence = 0L
$script:CurrentAgent = $Agent
$script:SessionId = ''

function Send-AgentEvent {
    param(
        [Parameter(Mandatory)]
        [string]$Type,

        [Parameter(Mandatory)]
        [System.Collections.IDictionary]$Fields,

        [Parameter(Mandatory)]
        [string]$Description
    )

    $timestamp = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    if ($script:Sequence -eq 0) {
        # Date.now() * 1000 + counter, as recommended by the protocol.
        $script:Sequence = $timestamp * 1000
    }
    $script:Sequence++

    $payload = [ordered]@{
        v     = 1
        agent = $script:CurrentAgent
        type  = $Type
        seq   = $script:Sequence
        ts    = $timestamp
    }
    foreach ($entry in $Fields.GetEnumerator()) {
        $payload[$entry.Key] = $entry.Value
    }

    $json = $payload | ConvertTo-Json -Compress -Depth 8
    $base64 = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($json))
    if ($base64.Length -gt 8192) {
        throw 'Encoded payload exceeds the OSC 9;7 size cap.'
    }

    [Console]::Out.Write("$ESC]9;7;$base64$BEL")
    [Console]::Out.Flush()
    Write-Host ("  {0,-10} {1} [{2}]" -f $Type, $Description, $script:CurrentAgent)

    if ($DelayMs -gt 0) {
        Start-Sleep -Milliseconds $DelayMs
    }
}

function Start-DemoAgent {
    param([Parameter(Mandatory)][string]$Id)

    if ($Id -notmatch '^[a-z][a-z0-9_-]*$') {
        throw "Invalid agent identifier: $Id"
    }

    $script:CurrentAgent = $Id
    $script:SessionId = "$Id-demo-$PID"
    Send-AgentEvent -Type session -Fields ([ordered]@{
        session_id = $script:SessionId
        reason     = 'startup'
    }) -Description "session $script:SessionId"
}

function Send-Model {
    param(
        [Parameter(Mandatory)][string]$Provider,
        [Parameter(Mandatory)][string]$ModelId,
        [Parameter(Mandatory)][string]$ModelName,
        [Parameter(Mandatory)][long]$ContextUsed,
        [Parameter(Mandatory)][bool]$Reasoning
    )

    Send-AgentEvent -Type model -Fields ([ordered]@{
        provider          = $Provider
        model_id          = $ModelId
        model_name        = $ModelName
        context_window    = 200000
        max_output_tokens = 8192
        reasoning         = $Reasoning
        source            = 'set'
        context_used      = $ContextUsed
    }) -Description "$ModelName, $ContextUsed/200000 tokens"
}

function Send-State {
    param(
        [Parameter(Mandatory)][string]$State,
        [Parameter(Mandatory)][string]$Message
    )

    Send-AgentEvent -Type state -Fields ([ordered]@{
        state      = $State
        message    = $Message
        session_id = $script:SessionId
    }) -Description "${State}: $Message"
}

function Invoke-DemoScenario {
    Start-DemoAgent $Agent
    Send-Model 'anthropic' 'claude-sonnet-4' 'Claude Sonnet 4' 84500 $true
    Send-State 'working' 'Analyzing the workspace'
    Send-AgentEvent -Type heartbeat -Fields ([ordered]@{
        interval_ms = 5000
        state       = 'working'
    }) -Description 'working keepalive'

    Send-AgentEvent -Type tool_call -Fields ([ordered]@{
        tool_call_id = 'tc-demo-1'
        tool         = 'bash'
        phase        = 'start'
        target       = 'src/app.rs'
        args         = 'grep -n TODO src/app.rs; cargo check'
        args_redacted = $false
    }) -Description 'bash started'
    Send-AgentEvent -Type tool_call -Fields ([ordered]@{
        tool_call_id = 'tc-demo-1'
        tool         = 'bash'
        phase        = 'update'
        progress     = 'Checking dependencies (42%)'
    }) -Description 'bash progress 42%'
    Send-AgentEvent -Type tool_call -Fields ([ordered]@{
        tool_call_id = 'tc-demo-1'
        tool         = 'bash'
        phase        = 'end'
        exit_code    = 0
        is_error     = $false
        duration_ms  = 900
        diff_stat    = '+12 -3'
    }) -Description 'bash completed'
    Send-AgentEvent -Type file -Fields ([ordered]@{
        path         = 'src/app.rs'
        action       = 'edit'
        tool_call_id = 'tc-demo-1'
    }) -Description 'edited src/app.rs'

    Send-State 'blocked' 'Waiting for permission'
    Send-AgentEvent -Type approval -Fields ([ordered]@{
        id           = 'apr-demo-1'
        kind         = 'permission'
        prompt       = 'Allow bash to run `cargo test --workspace`?'
        options      = @('yes', 'no', 'always')
        default      = 'no'
        tool         = 'bash'
        tool_call_id = 'tc-demo-2'
        risk         = 'medium'
        timeout_ms   = 0
    }) -Description 'approval requested'

    Send-State 'working' 'Permission received; continuing'
    Send-AgentEvent -Type model -Fields ([ordered]@{
        provider          = 'anthropic'
        model_id          = 'claude-sonnet-4'
        model_name        = 'Claude Sonnet 4'
        context_window    = 200000
        max_output_tokens = 8192
        reasoning         = $true
        source            = 'set'
        context_used      = 121000
    }) -Description 'context updated to 121000/200000'
    Send-State 'error' 'Simulated non-retryable provider error'
    Send-State 'idle' 'Ready for the next prompt'
}

function Invoke-WorkingScenario {
    Start-DemoAgent $Agent
    Send-Model 'anthropic' 'claude-sonnet-4' 'Claude Sonnet 4' 84500 $true
    Send-State 'working' 'Running workspace checks'
    Send-AgentEvent -Type tool_call -Fields ([ordered]@{
        tool_call_id  = 'tc-working-1'
        tool          = 'bash'
        phase         = 'start'
        target        = 'cargo test --workspace'
        args          = 'cargo test --workspace'
        args_redacted = $false
    }) -Description 'long-running workspace test'
    Send-AgentEvent -Type heartbeat -Fields ([ordered]@{
        interval_ms = 5000
        state       = 'working'
    }) -Description 'working keepalive'
}

function Invoke-BlockedScenario {
    Start-DemoAgent $Agent
    Send-Model 'openai' 'gpt-5-codex' 'GPT-5 Codex' 64000 $true
    Send-State 'blocked' 'Waiting for permission'
    Send-AgentEvent -Type approval -Fields ([ordered]@{
        id           = 'apr-blocked-1'
        kind         = 'permission'
        prompt       = 'Allow the agent to modify Cargo.toml?'
        options      = @('yes', 'no', 'always')
        default      = 'no'
        tool         = 'edit'
        tool_call_id = 'tc-blocked-1'
        risk         = 'high'
        timeout_ms   = 0
    }) -Description 'high-risk approval requested'
}

function Invoke-IdleScenario {
    Start-DemoAgent $Agent
    Send-Model 'anthropic' 'claude-sonnet-4' 'Claude Sonnet 4' 42000 $true
    Send-State 'idle' 'Ready for the next prompt'
}

function Invoke-ErrorScenario {
    Start-DemoAgent $Agent
    Send-Model 'openai' 'gpt-5-codex' 'GPT-5 Codex' 91000 $true
    Send-State 'error' 'Simulated authentication failure'
}

function Invoke-DoneScenario {
    Start-DemoAgent $Agent
    Send-Model 'local' 'demo-model' 'Demo Model' 12000 $false
    Send-State 'done' 'Session completed successfully'
}

function Invoke-MultiScenario {
    Start-DemoAgent 'pi'
    Send-Model 'anthropic' 'claude-sonnet-4' 'Claude Sonnet 4' 84500 $true
    Send-State 'working' 'Reviewing source code'
    Send-AgentEvent -Type tool_call -Fields ([ordered]@{
        tool_call_id  = 'tc-multi-pi'
        tool          = 'read'
        phase         = 'start'
        target        = 'crates/terminal/src/osc.rs'
        args          = 'Read OSC parser'
        args_redacted = $false
    }) -Description 'reading OSC parser'

    Start-DemoAgent 'codex'
    Send-Model 'openai' 'gpt-5-codex' 'GPT-5 Codex' 132000 $true
    Send-State 'blocked' 'Waiting for permission'
    Send-AgentEvent -Type approval -Fields ([ordered]@{
        id         = 'apr-multi-codex'
        kind       = 'confirm'
        prompt     = 'Run the full workspace quality gate?'
        options    = @('yes', 'no')
        default    = 'yes'
        tool       = 'bash'
        risk       = 'medium'
        timeout_ms = 0
    }) -Description 'confirmation requested'

    Start-DemoAgent 'claude'
    Send-Model 'anthropic' 'claude-opus-4' 'Claude Opus 4' 38000 $true
    Send-State 'idle' 'Ready for the next prompt'
}

Write-Host "OneTerm OSC 9;7 agent-status simulation: $Scenario"
switch ($Scenario) {
    'Demo'    { Invoke-DemoScenario }
    'Working' { Invoke-WorkingScenario }
    'Blocked' { Invoke-BlockedScenario }
    'Idle'    { Invoke-IdleScenario }
    'Error'   { Invoke-ErrorScenario }
    'Done'    { Invoke-DoneScenario }
    'Multi'   { Invoke-MultiScenario }
}
Write-Host 'Simulation complete. Open the Agent Panel to inspect the resulting card(s).'
