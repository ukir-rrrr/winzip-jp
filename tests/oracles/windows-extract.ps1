# tests/oracles/windows-extract.ps1
# 1) raw バイト: Python 3（ランナー標準）
#    zipfile は bit 11 なしを CP437 としてデコードする。
#    CP437 は 0x00-0xFF 全単射なので encode('cp437') で生バイトが戻る。
#    本リポジトリの inspect_zip.py はヘッダを自前パースする。
# 2) Unicode 名: 7-Zip に CP932 を明示
#
# 補助（失敗してもジョブは落とさない、レポートのみ）:
# - Expand-Archive — GitHub windows-2022 は en-US / ACP=1252 のため、
#   CP932 名を正しく復元しない。bit 11 が落ちていることのスモーク。
#   Expand-Archive が表示どおりの日本語を出した場合、それは bit 11 が
#   誤って立っている可能性を意味する（本製品では失敗に相当しうる）。
# - tar / Shell.Application — 同上、ACP 依存。参考レポートのみ。

param(
    [Parameter(Mandatory = $true)][string]$ZipPath,
    [Parameter(Mandatory = $true)][string]$ExpectPath,
    [Parameter(Mandatory = $true)][string]$WorkDir
)

$ErrorActionPreference = "Stop"

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$Inspect = Join-Path $RepoRoot "tests\oracles\inspect_zip.py"
$Compare = Join-Path $RepoRoot "tests\oracles\compare_extracted.py"

function Get-Python {
    foreach ($c in @("python", "python3", "py")) {
        $cmd = Get-Command $c -ErrorAction SilentlyContinue
        if ($cmd) { return $cmd.Source }
    }
    throw "Python が見つかりません"
}

function Get-SevenZip {
    $candidates = @(
        "C:\Program Files\7-Zip\7z.exe",
        "C:\Program Files (x86)\7-Zip\7z.exe"
    )
    foreach ($c in $candidates) {
        if (Test-Path $c) { return $c }
    }
    $cmd = Get-Command "7z" -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    throw "7z.exe が見つかりません（choco install 7zip -y）"
}

New-Item -ItemType Directory -Force -Path $WorkDir | Out-Null
$Python = Get-Python
$Seven = Get-SevenZip

Write-Host "inspect_zip.py"
& $Python $Inspect $ZipPath --expect $ExpectPath
if ($LASTEXITCODE -ne 0) {
    throw "inspect_zip.py failed"
}

$SevenOut = Join-Path $WorkDir "7z"
New-Item -ItemType Directory -Force -Path $SevenOut | Out-Null
Write-Host "7z x -mcp=932"
& $Seven x -y "-mcp=932" "-o$SevenOut" $ZipPath
if ($LASTEXITCODE -ne 0) {
    throw "7-Zip extract failed"
}

Write-Host "compare_extracted.py"
& $Python $Compare $SevenOut $ExpectPath
if ($LASTEXITCODE -ne 0) {
    throw "7-Zip Unicode names did not match expected.json"
}

function Get-ExpectedNfc {
    $json = Get-Content -Raw -Encoding UTF8 $ExpectPath | ConvertFrom-Json
    return @($json.entries | ForEach-Object { $_.name_nfc })
}

function Report-OptionalExtract {
    param(
        [string]$Label,
        [string]$OutDir,
        [scriptblock]$Extract
    )
    Write-Host "---- optional: $Label (must not fail the job) ----"
    try {
        New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
        & $Extract
        $want = Get-ExpectedNfc | Where-Object { $_ }
        $got = @()
        if (Test-Path $OutDir) {
            Get-ChildItem -Recurse -Force $OutDir | ForEach-Object {
                $rel = $_.FullName.Substring((Resolve-Path $OutDir).Path.Length).TrimStart('\')
                if (-not $rel) { return }
                $rel = $rel.Replace('\', '/')
                if ($_.PSIsContainer) { $rel = $rel.TrimEnd('/') + '/' }
                $got += $rel
            }
        }
        $gotSet = [System.Collections.Generic.HashSet[string]]::new([string[]]$got)
        $nonAscii = @($want | Where-Object { $_ -match '[^\x00-\x7F]' })
        if ($nonAscii.Count -eq 0) {
            Write-Host "$Label : ASCII のみなので bit 11 スモークの対象外。"
        } else {
            $japaneseOk = $true
            foreach ($n in $nonAscii) {
                if (-not $gotSet.Contains($n)) {
                    $japaneseOk = $false
                    break
                }
            }
            if ($japaneseOk) {
                Write-Host "WARNING: $Label が日本語を正しく展開した。bit 11 が誤って立っている可能性がある（本製品では失敗を意味しうる）。"
            } else {
                Write-Host "$Label : 日本語は UTF-8 として正しく展開されていない（bit 11 が落ちているスモークとしては期待どおり）。"
            }
        }
        Write-Host "  got: $($got -join ', ')"
    } catch {
        Write-Host "$Label optional report failed (ignored): $_"
    }
}

$ExpandDir = Join-Path $WorkDir "expand-archive"
Report-OptionalExtract -Label "Expand-Archive" -OutDir $ExpandDir -Extract {
    # Expand-Archive が表示どおりの日本語を出したら bit 11 誤爆の疑い。
    Expand-Archive -Path $ZipPath -DestinationPath $ExpandDir -Force
}

$TarDir = Join-Path $WorkDir "tar"
Report-OptionalExtract -Label "tar" -OutDir $TarDir -Extract {
    tar -xf $ZipPath -C $TarDir
}

$ShellDir = Join-Path $WorkDir "shell-application"
Report-OptionalExtract -Label "Shell.Application" -OutDir $ShellDir -Extract {
    $shell = New-Object -ComObject Shell.Application
    $zipItem = $shell.NameSpace((Resolve-Path $ZipPath).Path)
    $dest = $shell.NameSpace((Resolve-Path $ShellDir).Path)
    $dest.CopyHere($zipItem.Items(), 0x14)
    Start-Sleep -Seconds 2
}

Write-Host "windows-extract.ps1: required checks passed"
exit 0
