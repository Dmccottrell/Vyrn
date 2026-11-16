# Makes each project in this folder its own GitHub repository.
#
# Needs: Git, and the GitHub CLI signed in to your account:
#   winget install GitHub.cli
#   gh auth login
#
# Run it from this folder (vyrn\projects):
#   .\publish.ps1
#
# Each project is copied to $HOME\vyrn-projects\<name>, made into its own
# Git repository, and pushed to a new public repository vyrn-<name> on
# your GitHub account. Projects that already exist on GitHub are skipped.

$projects = "expenses", "adventure", "notes", "weather", "asteroids"
$target = Join-Path $HOME "vyrn-projects"
New-Item -ItemType Directory -Force $target | Out-Null

foreach ($name in $projects) {
    $repo = "vyrn-$name"
    Write-Host "== $repo ==" -ForegroundColor Cyan

    # Skip projects already on GitHub, so the script can be run again.
    gh repo view $repo *> $null
    if ($LASTEXITCODE -eq 0) {
        Write-Host "   already on GitHub, skipped"
        continue
    }

    # A clean copy of the project, outside this repository.
    $dest = Join-Path $target $name
    if (Test-Path $dest) { Remove-Item -Recurse -Force $dest }
    Copy-Item -Recurse $name $dest

    # Its own repository with one first commit...
    Push-Location $dest
    git init -q -b main
    git add .
    git commit -q -m "First version of $repo, written in Vyrn"

    # ...created on GitHub and pushed in one step.
    gh repo create $repo --public --source . --push --description "A Vyrn project: $name"
    Pop-Location
}

Write-Host "Done. Your repositories are at https://github.com/$(gh api user --jq .login)?tab=repositories" -ForegroundColor Green
