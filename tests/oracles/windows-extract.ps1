# tests/oracles/windows-extract.ps1
# Required:
#   1) inspect_zip.py -- raw name bytes + gpbf (custom header parse)
#   2) 7-Zip -mcp=932 -- Unicode names vs expected.json name_nfc
#
# Optional (must NOT fail the job): Expand-Archive / tar / Shell.Application
# GitHub windows-2022 is en-US (ACP 1252). CP932 names will not round-trip.
# That smoke checks bit 11 is down. If Expand-Archive shows correct Japanese,
# bit 11 may have been set by mistake (failure signal for this product).
#
# Encoding: UTF-8 with BOM + CRLF + ASCII-only source so Windows PowerShell 5.1
# (powershell.exe) can parse the script. Do not add non-ASCII to this file.

param(
    [Parameter(Mandatory = $true)][string]$ZipPath,
    [Parameter(Mandatory = $true)][string]$ExpectPath,
    [Parameter(Mandatory = $true)][string]$WorkDir
)

$ErrorActionPreference = 'Stop'

# PowerShell 5.1 leaves $PSScriptRoot empty when -File uses a \\?\ path.
function Convert-WinPath([string]$p) {
    if (-not $p) { return $p }
    if ($p.StartsWith('\\?\UNC\')) { return ('\\' + $p.Substring(8)) }
    if ($p.StartsWith('\\?\')) { return $p.Substring(4) }
    return $p
}

$ZipPath = Convert-WinPath $ZipPath
$ExpectPath = Convert-WinPath $ExpectPath
$WorkDir = Convert-WinPath $WorkDir

$scriptDir = $PSScriptRoot
if (-not $scriptDir) {
    $scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
}
$scriptDir = Convert-WinPath $scriptDir
$RepoRoot = Resolve-Path (Join-Path $scriptDir '..\..')
$Inspect = Join-Path $RepoRoot 'tests\oracles\inspect_zip.py'
$Compare = Join-Path $RepoRoot 'tests\oracles\compare_extracted.py'

function Get-Python {
    foreach ($c in @('python', 'python3', 'py')) {
        $cmd = Get-Command $c -ErrorAction SilentlyContinue
        if ($cmd) { return [string]$cmd.Source }
    }
    throw 'Python not found'
}

function Get-SevenZip {
    $candidates = @(
        'C:\Program Files\7-Zip\7z.exe',
        'C:\Program Files (x86)\7-Zip\7z.exe'
    )
    foreach ($c in $candidates) {
        if (Test-Path -LiteralPath $c) { return $c }
    }
    $cmd = Get-Command '7z' -ErrorAction SilentlyContinue
    if ($cmd) { return [string]$cmd.Source }
    throw '7z.exe not found. Run: choco install 7zip -y'
}

New-Item -ItemType Directory -Force -Path $WorkDir | Out-Null
$Python = Get-Python
$Seven = Get-SevenZip

Write-Host 'inspect_zip.py'
& "$Python" "$Inspect" "$ZipPath" '--expect' "$ExpectPath"
if ($LASTEXITCODE -ne 0) {
    throw 'inspect_zip.py failed'
}

$SevenOut = Join-Path $WorkDir '7z'
New-Item -ItemType Directory -Force -Path $SevenOut | Out-Null
$expectObj = Get-Content -Raw -Encoding UTF8 -LiteralPath $ExpectPath | ConvertFrom-Json
$password = $null
if ($null -ne $expectObj.password -and "$($expectObj.password)" -ne '') {
    $password = [string]$expectObj.password
}
Write-Host '7z x -mcp=932'
$outSwitch = '-o' + $SevenOut
if ($password) {
    & "$Seven" x -y '-mcp=932' ("-p" + $password) $outSwitch "$ZipPath"
} else {
    & "$Seven" x -y '-mcp=932' $outSwitch "$ZipPath"
}
if ($LASTEXITCODE -ne 0) {
    throw '7-Zip extract failed'
}

Write-Host 'compare_extracted.py'
& "$Python" "$Compare" "$SevenOut" "$ExpectPath"
if ($LASTEXITCODE -ne 0) {
    throw '7-Zip Unicode names did not match expected.json'
}

function Get-ExpectedNfc {
    $json = Get-Content -Raw -Encoding UTF8 -LiteralPath $ExpectPath | ConvertFrom-Json
    return @($json.entries | ForEach-Object { $_.name_nfc })
}

function Report-OptionalExtract {
    param(
        [string]$Label,
        [string]$OutDir,
        [scriptblock]$Extract
    )
    Write-Host ("---- optional: {0} (must not fail the job) ----" -f $Label)
    try {
        New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
        & $Extract
        $want = Get-ExpectedNfc | Where-Object { $_ }
        $got = @()
        if (Test-Path -LiteralPath $OutDir) {
            Get-ChildItem -Recurse -Force -LiteralPath $OutDir | ForEach-Object {
                $root = (Resolve-Path -LiteralPath $OutDir).Path
                $rel = $_.FullName.Substring($root.Length).TrimStart('\')
                if (-not $rel) { return }
                $rel = $rel.Replace('\', '/')
                if ($_.PSIsContainer) { $rel = $rel.TrimEnd('/') + '/' }
                $got += $rel
            }
        }
        $gotSet = New-Object 'System.Collections.Generic.HashSet[string]'
        foreach ($g in $got) { [void]$gotSet.Add($g) }
        $nonAscii = @($want | Where-Object { $_ -match '[^\x00-\x7F]' })
        if ($nonAscii.Count -eq 0) {
            Write-Host ("{0}: ASCII-only fixture; bit-11 smoke N/A." -f $Label)
        } else {
            $japaneseOk = $true
            foreach ($n in $nonAscii) {
                if (-not $gotSet.Contains($n)) {
                    $japaneseOk = $false
                    break
                }
            }
            if ($japaneseOk) {
                Write-Host ("WARNING: {0} extracted Japanese names correctly. bit 11 may be set by mistake (failure signal for this product)." -f $Label)
            } else {
                Write-Host ("{0}: Japanese was NOT restored as UTF-8 (expected smoke when bit 11 is down)." -f $Label)
            }
        }
        Write-Host ("  got: {0}" -f ($got -join ', '))
    } catch {
        Write-Host ("{0} optional report failed (ignored): {1}" -f $Label, $_)
    }
}

$ExpandDir = Join-Path $WorkDir 'expand-archive'
Report-OptionalExtract -Label 'Expand-Archive' -OutDir $ExpandDir -Extract {
    Expand-Archive -LiteralPath $ZipPath -DestinationPath $ExpandDir -Force
}

$TarDir = Join-Path $WorkDir 'tar'
Report-OptionalExtract -Label 'tar' -OutDir $TarDir -Extract {
    tar -xf $ZipPath -C $TarDir
}

$ShellDir = Join-Path $WorkDir 'shell-application'
Report-OptionalExtract -Label 'Shell.Application' -OutDir $ShellDir -Extract {
    $shell = New-Object -ComObject Shell.Application
    $zipItem = $shell.NameSpace((Resolve-Path -LiteralPath $ZipPath).Path)
    $destNs = $shell.NameSpace((Resolve-Path -LiteralPath $ShellDir).Path)
    $destNs.CopyHere($zipItem.Items(), 0x14)
    Start-Sleep -Seconds 2
}

Write-Host 'windows-extract.ps1: required checks passed'
exit 0
