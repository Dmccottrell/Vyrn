# install-plugin.ps1: puts the VyrnScripting plugin into an Unreal project.
#
#   .\unreal\install-plugin.ps1 -Project "C:\Users\me\Documents\Unreal Projects\MyShooter"
#
# It builds the Vyrn runtime library (vyrn.dll), copies the plugin into the
# project's Plugins folder with the library inside, and copies the example
# scripts and unreal.vyrn into Content\Scripts (never overwriting a script
# you already have there). Run it from the Vyrn repository folder. Run it
# again after updating Vyrn; close the Unreal Editor first, because it
# locks vyrn.dll while open.

param(
    # The folder holding your .uproject file.
    [Parameter(Mandatory = $true)][string]$Project
)

$ErrorActionPreference = "Stop"
$Repo = Split-Path -Parent $PSScriptRoot

if (-not (Get-ChildItem -Path $Project -Filter *.uproject -ErrorAction SilentlyContinue)) {
    throw "No .uproject file in '$Project'. Give the folder that holds your project's .uproject file."
}

Write-Host "1/3 Building the Vyrn runtime library (vyrn.dll)..."
cargo build --release --manifest-path "$Repo\runtime\embed\Cargo.toml"
if ($LASTEXITCODE -ne 0) { throw "cargo build failed (see above)." }

Write-Host "2/3 Copying the plugin into $Project\Plugins\VyrnScripting..."
$Plugin = Join-Path $Project "Plugins\VyrnScripting"
New-Item -ItemType Directory -Force $Plugin | Out-Null
# Copy fresh, but keep the build folders Unreal made, so it needn't rebuild everything.
Copy-Item -Recurse -Force "$Repo\unreal\VyrnScripting\*" $Plugin -Exclude Scripts
New-Item -ItemType Directory -Force "$Plugin\Source\ThirdParty\Vyrn\Win64" | Out-Null
Copy-Item -Force "$Repo\runtime\embed\include\vyrn.h" "$Plugin\Source\ThirdParty\Vyrn\include\"
Copy-Item -Force "$Repo\runtime\embed\target\release\vyrn.dll" "$Plugin\Source\ThirdParty\Vyrn\Win64\"
Copy-Item -Force "$Repo\runtime\embed\target\release\vyrn.dll.lib" "$Plugin\Source\ThirdParty\Vyrn\Win64\"
# The editor loads the plugin's DLLs from Binaries\Win64.
New-Item -ItemType Directory -Force "$Plugin\Binaries\Win64" | Out-Null
Copy-Item -Force "$Repo\runtime\embed\target\release\vyrn.dll" "$Plugin\Binaries\Win64\"

Write-Host "3/3 Copying scripts into $Project\Content\Scripts..."
$Scripts = Join-Path $Project "Content\Scripts"
New-Item -ItemType Directory -Force $Scripts | Out-Null
# unreal.vyrn always updates (it must match the plugin); your scripts are never replaced.
Copy-Item -Force "$Repo\unreal\VyrnScripting\Scripts\unreal.vyrn" $Scripts
Get-ChildItem "$Repo\unreal\VyrnScripting\Scripts\*.vyrn" | Where-Object { $_.Name -ne "unreal.vyrn" } | ForEach-Object {
    $Target = Join-Path $Scripts $_.Name
    if (-not (Test-Path $Target)) { Copy-Item $_.FullName $Target }
}

Write-Host ""
Write-Host "Done. Next:"
Write-Host "  1. Open the project. If Unreal asks to rebuild the VyrnScripting module, choose Yes."
Write-Host "     (A Blueprint-only project needs one C++ class first: Tools > New C++ Class > None > Create.)"
Write-Host "  2. Select an actor, Add Component > Vyrn Script, and set Script to spinner.vyrn."
Write-Host "  3. Press Play. Edit $Scripts\spinner.vyrn and save: the change shows at once."
