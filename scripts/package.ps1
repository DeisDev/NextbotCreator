param(
    [switch]$SkipFfmpeg
)

$ErrorActionPreference = 'Stop'
$workspace = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$manifest = Join-Path $workspace 'Cargo.toml'
$version = ((cargo metadata --manifest-path $manifest --no-deps --format-version 1 | ConvertFrom-Json).packages | Where-Object { $_.name -eq 'nextbot-creator' } | Select-Object -First 1).version
if (-not $version) { throw 'Could not read the package version from Cargo.toml' }
$distRoot = Join-Path $workspace 'dist'
$bundle = Join-Path $distRoot "NextbotCreator-$version-windows-x64"
$cache = Join-Path $workspace 'vendor\tools\cache'
$ffmpegArchive = Join-Path $cache 'ffmpeg-8.1.2-essentials_build.zip'
$ffmpegUrl = 'https://www.gyan.dev/ffmpeg/builds/packages/ffmpeg-8.1.2-essentials_build.zip'
$ffmpegSha256 = 'db580001caa24ac104c8cb856cd113a87b0a443f7bdf47d8c12b1d740584a2ec'

Push-Location $workspace
try {
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }

    New-Item -ItemType Directory -Force -Path $distRoot | Out-Null
    if (Test-Path $bundle) {
        $resolvedBundle = (Resolve-Path $bundle).Path
        $resolvedDist = (Resolve-Path $distRoot).Path
        if (-not $resolvedBundle.StartsWith($resolvedDist, [System.StringComparison]::OrdinalIgnoreCase)) {
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
        $extract = Join-Path $env:TEMP "nextbotcreator-ffmpeg-$version"
        if (Test-Path $extract) {
            $resolvedExtract = (Resolve-Path $extract).Path
            $resolvedTemp = (Resolve-Path $env:TEMP).Path
            if (-not $resolvedExtract.StartsWith($resolvedTemp, [System.StringComparison]::OrdinalIgnoreCase)) {
                throw "Refusing to clear a folder outside the temporary directory: $resolvedExtract"
            }
            Remove-Item -LiteralPath $resolvedExtract -Recurse -Force
        }
        Expand-Archive -LiteralPath $ffmpegArchive -DestinationPath $extract
        $ffmpeg = Get-ChildItem -Path $extract -Recurse -File -Filter 'ffmpeg.exe' | Select-Object -First 1
        if (-not $ffmpeg) { throw 'ffmpeg.exe was not found in the verified archive' }
        Copy-Item -LiteralPath $ffmpeg.FullName -Destination (Join-Path $bundle 'tools\ffmpeg.exe')
        $buildReadme = Get-ChildItem -Path $extract -Recurse -File -Filter 'README.txt' | Select-Object -First 1
        if ($buildReadme) {
            Copy-Item -LiteralPath $buildReadme.FullName -Destination (Join-Path $bundle 'tools\ffmpeg-build-info.txt')
        }
        $ffmpegLicense = Get-ChildItem -Path $extract -Recurse -File | Where-Object { $_.Name -match '^(COPYING|LICENSE)(\.txt)?$' } | Select-Object -First 1
        if ($ffmpegLicense) {
            Copy-Item -LiteralPath $ffmpegLicense.FullName -Destination (Join-Path $bundle 'tools\ffmpeg-license.txt')
        }
        Remove-Item -LiteralPath $extract -Recurse -Force
    }

    Write-Host "Portable bundle created at $bundle"
}
finally {
    Pop-Location
}
