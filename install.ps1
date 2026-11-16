# Installs the latest Vyrn release on Windows:
#
#   irm https://raw.githubusercontent.com/dmccottrell/vyrn/main/install.ps1 | iex
#
# Puts vyrn.exe in %USERPROFILE%\.vyrn\bin and adds that to your PATH.
# Set $env:VYRN_VERSION = "v0.4.0" for a particular release.
$ErrorActionPreference = "Stop"
$repo = "dmccottrell/vyrn"
$dir = Join-Path $env:USERPROFILE ".vyrn\bin"
$version = if ($env:VYRN_VERSION) { $env:VYRN_VERSION } else {
    (Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest").tag_name
}
$name = "vyrn-$version-x86_64-pc-windows-msvc"
$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid())
New-Item -ItemType Directory $tmp | Out-Null
try {
    Write-Host "downloading $name"
    $zip = Join-Path $tmp "vyrn.zip"
    Invoke-WebRequest "https://github.com/$repo/releases/download/$version/$name.zip" -OutFile $zip
    Expand-Archive $zip -DestinationPath $tmp
    New-Item -ItemType Directory -Force $dir | Out-Null
    Copy-Item (Join-Path $tmp "$name\vyrn.exe") (Join-Path $dir "vyrn.exe") -Force
} finally {
    Remove-Item -Recurse -Force $tmp
}
$path = [Environment]::GetEnvironmentVariable("Path", "User")
if (-not ($path -split ";" | Where-Object { $_ -eq $dir })) {
    [Environment]::SetEnvironmentVariable("Path", "$dir;$path", "User")
    Write-Host "added $dir to your PATH (open a new terminal to use it)"
}
& (Join-Path $dir "vyrn.exe") --version
Write-Host "then try:  vyrn        (the interactive prompt)"
