param(
    [Alias('SkipTools')][switch]$SkipFfmpeg
)

$ErrorActionPreference = 'Stop'
$workspace = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$manifest = Join-Path $workspace 'Cargo.toml'
$version = ((cargo metadata --manifest-path $manifest --no-deps --format-version 1 | ConvertFrom-Json).packages | Where-Object { $_.name -eq 'nextbot-creator' } | Select-Object -First 1).version
if (-not $version) { throw 'Could not read the package version from Cargo.toml' }
$distRoot = Join-Path $workspace 'dist'
$bundle = Join-Path $distRoot "NextbotCreator-$version-windows-x64"
if ($SkipFfmpeg) { $bundle += '-app-only' }
$cache = Join-Path $workspace 'vendor\tools\cache'
$ffmpegArchive = Join-Path $cache 'ffmpeg-8.1.2-essentials_build.zip'
$ffmpegUrl = 'https://www.gyan.dev/ffmpeg/builds/packages/ffmpeg-8.1.2-essentials_build.zip'
$ffmpegSha256 = 'db580001caa24ac104c8cb856cd113a87b0a443f7bdf47d8c12b1d740584a2ec'
$ytDlpVersion = '2026.08.19'
$ytDlpSha256 = '66674953fe251b89f4d08c5f0e35e0728679bd67ab3d7d05c0562af101dd3e7a'
$denoVersion = '2.9.6'
$denoSha256 = '15e5300b0ba3c3695a7621d90160a746ec9e710228cee639afa9d580f6e3cd11'

function Get-VerifiedTool([string]$Url, [string]$Destination, [string]$Sha256) {
    if (-not (Test-Path -LiteralPath $Destination)) {
        $partial = "$Destination.download"
        try {
            Invoke-WebRequest -UseBasicParsing -Uri $Url -OutFile $partial
            if ((Get-FileHash -Algorithm SHA256 -LiteralPath $partial).Hash.ToLowerInvariant() -ne $Sha256) { throw "Tool checksum mismatch: $Url" }
            Move-Item -LiteralPath $partial -Destination $Destination
        }
        finally { if (Test-Path -LiteralPath $partial) { Remove-Item -LiteralPath $partial -Force } }
    }
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath $Destination).Hash.ToLowerInvariant() -ne $Sha256) { throw "Cached tool checksum mismatch: $Destination" }
}

function Remove-ToolStaging([string]$Directory) {
    if (Test-Path -LiteralPath $Directory) {
        $resolvedDirectory = (Resolve-Path -LiteralPath $Directory).Path
        $resolvedCache = (Resolve-Path -LiteralPath $cache).Path
        if (-not $resolvedDirectory.StartsWith($resolvedCache + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)) { throw "Refusing to clear a folder outside the cache: $resolvedDirectory" }
        Remove-Item -LiteralPath $resolvedDirectory -Recurse -Force
    }
}

Push-Location $workspace
try {
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }

    New-Item -ItemType Directory -Force -Path $distRoot | Out-Null
    if (Test-Path $bundle) {
        $resolvedBundle = (Resolve-Path $bundle).Path
        $resolvedDist = (Resolve-Path $distRoot).Path
        if (-not $resolvedBundle.StartsWith($resolvedDist + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)) {
            throw "Refusing to replace a folder outside dist: $resolvedBundle"
        }
        Remove-Item -LiteralPath $resolvedBundle -Recurse -Force
    }
    New-Item -ItemType Directory -Force -Path (Join-Path $bundle 'tools') | Out-Null

    Copy-Item -LiteralPath 'target\release\nextbot-creator.exe' -Destination (Join-Path $bundle 'NextbotCreator.exe')
    Copy-Item -LiteralPath 'README.md','CHANGELOG.md','LICENSE','THIRD_PARTY_NOTICES.txt' -Destination $bundle

    if (-not $SkipFfmpeg) {
        New-Item -ItemType Directory -Force -Path $cache | Out-Null
        if (-not (Test-Path $ffmpegArchive)) {
            $download = "$ffmpegArchive.download"
            if (Test-Path $download) {
                Remove-Item -LiteralPath $download -Force
            }
            try {
                Invoke-WebRequest -Uri $ffmpegUrl -OutFile $download
                $downloadHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $download).Hash.ToLowerInvariant()
                if ($downloadHash -ne $ffmpegSha256) {
                    throw "FFmpeg checksum mismatch. Expected $ffmpegSha256, got $downloadHash"
                }
                Move-Item -LiteralPath $download -Destination $ffmpegArchive
            }
            finally {
                if (Test-Path $download) {
                    Remove-Item -LiteralPath $download -Force
                }
            }
        }
        $actualHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $ffmpegArchive).Hash.ToLowerInvariant()
        if ($actualHash -ne $ffmpegSha256) {
            throw "FFmpeg checksum mismatch. Expected $ffmpegSha256, got $actualHash"
        }
        $extract = Join-Path $cache "ffmpeg-staging-$version"
        Remove-ToolStaging $extract
        Expand-Archive -LiteralPath $ffmpegArchive -DestinationPath $extract
        $ffmpeg = Get-ChildItem -Path $extract -Recurse -File -Filter 'ffmpeg.exe' | Select-Object -First 1
        if (-not $ffmpeg) { throw 'ffmpeg.exe was not found in the verified archive' }
        Copy-Item -LiteralPath $ffmpeg.FullName -Destination (Join-Path $bundle 'tools\ffmpeg.exe')
        $ffprobe = Get-ChildItem -LiteralPath $extract -Recurse -File -Filter 'ffprobe.exe' | Select-Object -First 1
        if (-not $ffprobe) { throw 'ffprobe.exe was not found in the verified archive' }
        Copy-Item -LiteralPath $ffprobe.FullName -Destination (Join-Path $bundle 'tools\ffprobe.exe')
        $buildReadme = Get-ChildItem -Path $extract -Recurse -File -Filter 'README.txt' | Select-Object -First 1
        if ($buildReadme) {
            Copy-Item -LiteralPath $buildReadme.FullName -Destination (Join-Path $bundle 'tools\ffmpeg-build-info.txt')
        }
        $ffmpegLicense = Get-ChildItem -Path $extract -Recurse -File | Where-Object { $_.Name -match '^(COPYING|LICENSE)(\.txt)?$' } | Select-Object -First 1
        if ($ffmpegLicense) {
            Copy-Item -LiteralPath $ffmpegLicense.FullName -Destination (Join-Path $bundle 'tools\ffmpeg-license.txt')
        }
        Remove-ToolStaging $extract

        $ytDlpCached = Join-Path $cache "yt-dlp-$ytDlpVersion.exe"
        Get-VerifiedTool "https://github.com/yt-dlp/yt-dlp/releases/download/$ytDlpVersion/yt-dlp.exe" $ytDlpCached $ytDlpSha256
        Copy-Item -LiteralPath $ytDlpCached -Destination (Join-Path $bundle 'tools\yt-dlp.exe')
        $ytDlpVersion | Set-Content -LiteralPath (Join-Path $bundle 'tools\yt-dlp-version.txt') -Encoding ASCII
        Invoke-WebRequest -UseBasicParsing -Uri "https://raw.githubusercontent.com/yt-dlp/yt-dlp/$ytDlpVersion/LICENSE" -OutFile (Join-Path $bundle 'tools\yt-dlp-license.txt')
        Invoke-WebRequest -UseBasicParsing -Uri "https://raw.githubusercontent.com/yt-dlp/yt-dlp/$ytDlpVersion/THIRD_PARTY_LICENSES.txt" -OutFile (Join-Path $bundle 'tools\yt-dlp-third-party-licenses.txt')

        $denoArchive = Join-Path $cache "deno-$denoVersion-windows-x64.zip"
        Get-VerifiedTool "https://github.com/denoland/deno/releases/download/v$denoVersion/deno-x86_64-pc-windows-msvc.zip" $denoArchive $denoSha256
        $denoExtract = Join-Path $cache "deno-staging-$version"
        Remove-ToolStaging $denoExtract
        try {
            Expand-Archive -LiteralPath $denoArchive -DestinationPath $denoExtract
            Copy-Item -LiteralPath (Join-Path $denoExtract 'deno.exe') -Destination (Join-Path $bundle 'tools\deno.exe')
        }
        finally { Remove-ToolStaging $denoExtract }
        Invoke-WebRequest -UseBasicParsing -Uri "https://raw.githubusercontent.com/denoland/deno/v$denoVersion/LICENSE.md" -OutFile (Join-Path $bundle 'tools\deno-license.txt')
        $denoNotices = Join-Path $bundle 'tools\deno-third-party-licenses.txt'
        'Notices for the V8 engine and TypeScript compiler bundled with Deno 2.9.6.' | Set-Content -LiteralPath $denoNotices -Encoding UTF8
        foreach ($noticeUrl in @(
            'https://raw.githubusercontent.com/denoland/rusty_v8/v150.4.0/LICENSE',
            'https://raw.githubusercontent.com/v8/v8/15.0.245.2/LICENSE',
            'https://raw.githubusercontent.com/microsoft/TypeScript/v6.0.3/LICENSE.txt',
            'https://raw.githubusercontent.com/microsoft/TypeScript/v6.0.3/ThirdPartyNoticeText.txt'
        )) {
            $noticeText = Invoke-RestMethod -Uri $noticeUrl
            "`r`n$noticeUrl`r`n$noticeText" | Add-Content -LiteralPath $denoNotices -Encoding UTF8
        }
    }

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = "$bundle.zip"
    $zipStaging = "$zip.partial"
    try {
        if (Test-Path -LiteralPath $zipStaging) { Remove-Item -LiteralPath $zipStaging -Force }
        [System.IO.Compression.ZipFile]::CreateFromDirectory(
            $bundle, $zipStaging, [System.IO.Compression.CompressionLevel]::Optimal, $true
        )
        Move-Item -LiteralPath $zipStaging -Destination $zip -Force
    }
    finally {
        if (Test-Path -LiteralPath $zipStaging) { Remove-Item -LiteralPath $zipStaging -Force }
    }
    $zipHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $zip).Hash.ToLowerInvariant()
    "$zipHash  $([System.IO.Path]::GetFileName($zip))" | Set-Content -LiteralPath "$zip.sha256" -Encoding ASCII
    $zipMiB = [Math]::Round((Get-Item -LiteralPath $zip).Length / 1MB, 2)
    Write-Host "Portable bundle created at $bundle"
    Write-Host "Release archive created at $zip ($zipMiB MiB), with SHA-256 checksum"
}
finally {
    Pop-Location
}
