param([string]$OutputDirectory = (Join-Path $PSScriptRoot '..\target\debug'))
$ErrorActionPreference = 'Stop'
$archive = Join-Path ([IO.Path]::GetTempPath()) ("rustyfin-mpv-" + [guid]::NewGuid() + ".7z")
$url = 'https://github.com/shinchiro/mpv-winbuild-cmake/releases/download/20261111/mpv-dev-x86_64-20261111-git-bcde6b93cc.7z'
try {
    Invoke-WebRequest $url -OutFile $archive
    $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $archive).Hash
    if ($hash -ne '273c079b11ea5575822f972756f4c9a71149a5c10130353abeb8586738938452') {
        throw 'The downloaded libmpv archive failed SHA-256 verification.'
    }
    $entries = @(tar.exe -tf $archive | Where-Object { $_ -match '^(\./)?(libmpv|mpv)-2\.dll$' })
    if ($LASTEXITCODE -ne 0 -or $entries.Count -ne 1) { throw 'Could not identify the libmpv DLL.' }
    [IO.Directory]::CreateDirectory($OutputDirectory) | Out-Null
    tar.exe -xf $archive -C $OutputDirectory $entries[0]
    if ($LASTEXITCODE -ne 0) { throw 'Could not extract libmpv.' }
    Get-Item -LiteralPath (Join-Path $OutputDirectory $entries[0])
} finally {
    if (Test-Path -LiteralPath $archive) { Remove-Item -LiteralPath $archive }
}
