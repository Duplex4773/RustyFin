param([string]$OutputDirectory = (Join-Path $PSScriptRoot '..\target\debug'))

$ErrorActionPreference = 'Stop'
$source = (Resolve-Path (Join-Path $PSScriptRoot '..\native')).Path
$destination = [IO.Path]::GetFullPath((Join-Path $OutputDirectory 'media'))
$build = [IO.Directory]::CreateDirectory((Join-Path $PSScriptRoot '..\target\native-media')).FullName
$archive = Join-Path $build 'ffmpeginteropx.desktop.ffmpeg.8.1.2.nupkg'
$hash = '9C291024F6C599C90D8B5898F0CF9D288C239CE770E8B781B426B403DF75091F'
$url = 'https://api.nuget.org/v3-flatcontainer/ffmpeginteropx.desktop.ffmpeg/8.1.2/ffmpeginteropx.desktop.ffmpeg.8.1.2.nupkg'
if (!(Test-Path -LiteralPath $archive) -or (Get-FileHash -LiteralPath $archive).Hash -ne $hash) {
    Invoke-WebRequest $url -OutFile $archive
}
if ((Get-FileHash -LiteralPath $archive).Hash -ne $hash) { throw 'FFmpeg package digest mismatch.' }

$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (!$vs) { throw 'Install the Visual Studio x64 C++ build tools and Windows SDK.' }
& (Join-Path $vs 'Common7\Tools\Launch-VsDevShell.ps1') -VsInstallationPath $vs -Arch amd64 -HostArch amd64 -SkipAutomaticLocation

Push-Location -LiteralPath $build
try {
    tar.exe -xf $archive include runtimes/win-x64/native licenses
    if ($LASTEXITCODE -ne 0) { throw 'Could not extract the FFmpeg development package.' }
    $libs = Join-Path $build 'runtimes\win-x64\native'
    $sources = @('source.cpp','video_type.cpp','bitstream.cpp','input.cpp') | ForEach-Object { Join-Path $source $_ }
    & cl.exe /nologo /std:c++20 /permissive- /EHsc /bigobj /MD /O2 /LD /DNOMINMAX /Iinclude @sources /Fe:rustyfin-media.dll /link "/LIBPATH:$libs" avcodec.lib avformat.lib avutil.lib swresample.lib shlwapi.lib mfplat.lib mfuuid.lib mf.lib ole32.lib ksuser.lib uuid.lib windowsapp.lib
    if ($LASTEXITCODE -ne 0) { throw 'Could not build the RustyFin Media Foundation source.' }
    [IO.Directory]::CreateDirectory($destination) | Out-Null
    foreach ($name in @('avcodec-62.dll','avformat-62.dll','avutil-60.dll','swresample-6.dll')) {
        Copy-Item -LiteralPath (Join-Path $libs $name) -Destination $destination -Force
    }
    Copy-Item -LiteralPath 'rustyfin-media.dll' -Destination $destination -Force
    $licenses = [IO.Directory]::CreateDirectory((Join-Path $destination 'licenses')).FullName
    Get-ChildItem -LiteralPath (Join-Path $build 'licenses') -File | Copy-Item -Destination $licenses -Force
    Get-Item -LiteralPath (Join-Path $destination 'rustyfin-media.dll')
} finally {
    Pop-Location
}
