# Install exodus on Windows; with a one-time code from the cabinet, also link the repository you are in to your
# Exodus account.
#
#   irm https://exodus.elim.io/install.ps1 | iex                                              install only
#   & ([scriptblock]::Create((irm https://exodus.elim.io/install.ps1))) -Code CODE [-New]    install, then
#                                                                         `exodus init --code CODE` here
#
# Downloads the release binary for this machine from GitHub Releases of elim-io/exodus, checks its SHA-256 against
# the release's SHA256SUMS, puts it in %LOCALAPPDATA%\Programs\exodus and adds that folder to your user PATH (no
# administrator rights). With a code it then runs `exodus init --code CODE` in the current folder, which links it only
# when it is the top folder of a git repository (or, with -New, an empty folder). Anywhere else, the home folder
# included, it leaves the code unused and prints the command that spends it in the right folder.
#
#   -Code CODE          the one-time code from the cabinet
#   -New                plan a new product in an empty folder (`exodus init --new`)
#   -Here               link this folder even when it is not a repository's top folder (`exodus init --here`)
#   -Api URL            another platform address (`exodus init --api URL`)
#   EXODUS_VERSION      a release tag, such as v0.2.9 (default: the latest release)
#   EXODUS_INSTALL_DIR  where to put the binary (default: %LOCALAPPDATA%\Programs\exodus)
#   GITHUB_TOKEN        read through the GitHub API with this token (a private fork, or rate limits)
param(
    [string]$Code = "",
    [switch]$New,
    [switch]$Here,
    [string]$Api = ""
)
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"
# Windows PowerShell 5.1 may default to TLS 1.0, which GitHub refuses
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

# no `exit` anywhere: run through `irm | iex`, an exit would close the person's PowerShell window
function Say([string]$m) { [Console]::Error.WriteLine("exodus: $m") }
function Die([string]$m) { throw "exodus: $m" }

$repo = if ($env:EXODUS_REPO) { $env:EXODUS_REPO } else { "elim-io/exodus" }
$version = if ($env:EXODUS_VERSION) { $env:EXODUS_VERSION } else { "latest" }
$dir = if ($env:EXODUS_INSTALL_DIR) { $env:EXODUS_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "Programs\exodus" }

# the machine's architecture, not the shell's: x64 PowerShell on an ARM64 machine still gets the ARM64 binary
$arch = $env:PROCESSOR_ARCHITEW6432
if (-not $arch) { $arch = $env:PROCESSOR_ARCHITECTURE }
switch ($arch) {
    "AMD64" { $target = "x86_64-pc-windows-msvc" }
    "ARM64" { $target = "aarch64-pc-windows-msvc" }
    default { Die "no release for Windows $arch; build from source: https://github.com/$repo" }
}
$archive = "exodus-$target.zip"

$tmp = Join-Path ([IO.Path]::GetTempPath()) ("exodus-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tmp | Out-Null

try {
    $headers = @{}
    if ($env:GITHUB_TOKEN) {
        # the API serves a private repository's assets; release/download URLs do not take a token
        $headers["Authorization"] = "Bearer $env:GITHUB_TOKEN"
        $rel = if ($version -eq "latest") { "latest" } else { "tags/$version" }
        try {
            $release = Invoke-RestMethod -Headers $headers -Uri "https://api.github.com/repos/$repo/releases/$rel"
        } catch { Die "no release $version in $repo" }
        function Fetch([string]$name) {
            $asset = $release.assets | Where-Object { $_.name -eq $name } | Select-Object -First 1
            if (-not $asset) { Die "the release has no $name" }
            $h = $headers.Clone(); $h["Accept"] = "application/octet-stream"
            try { Invoke-WebRequest -UseBasicParsing -Headers $h -Uri $asset.url -OutFile (Join-Path $tmp $name) }
            catch { Die "could not download $name" }
        }
    } else {
        $base = if ($version -eq "latest") { "https://github.com/$repo/releases/latest/download" }
                else { "https://github.com/$repo/releases/download/$version" }
        function Fetch([string]$name) {
            try { Invoke-WebRequest -UseBasicParsing -Uri "$base/$name" -OutFile (Join-Path $tmp $name) }
            catch { Die "could not download $base/$name" }
        }
    }

    Say "downloading $archive ($version)"
    Fetch $archive
    Fetch "SHA256SUMS"
    $want = $null
    foreach ($line in Get-Content (Join-Path $tmp "SHA256SUMS")) {
        $parts = $line -split "\s+", 2
        if ($parts.Count -eq 2 -and ($parts[1] -eq $archive -or $parts[1] -eq "*$archive")) { $want = $parts[0].ToLower() }
    }
    if (-not $want) { Die "SHA256SUMS has no line for $archive" }
    $got = (Get-FileHash -Algorithm SHA256 (Join-Path $tmp $archive)).Hash.ToLower()
    if ($got -ne $want) { Die "SHA-256 mismatch for $archive`: expected $want, got $got" }

    Expand-Archive -Path (Join-Path $tmp $archive) -DestinationPath (Join-Path $tmp "x") -Force
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    $exe = Join-Path $dir "exodus.exe"
    # a running exodus.exe cannot be overwritten but can be renamed away
    if (Test-Path $exe) {
        $old = Join-Path $dir "exodus.old.exe"
        Remove-Item -Force $old -ErrorAction SilentlyContinue
        Move-Item -Force $exe $old
    }
    Copy-Item (Join-Path $tmp "x\exodus.exe") $exe
    Say "installed $(& $exe --version) to $exe"
} finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}

# the user's PATH (no administrator rights), and this session's, so the next command finds exodus
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
$entries = @($userPath -split ";" | Where-Object { $_ })
if ($entries -notcontains $dir) {
    [Environment]::SetEnvironmentVariable("Path", (($entries + $dir) -join ";"), "User")
    Say "added $dir to your PATH; new terminals find exodus, this one does too"
}
if (($env:Path -split ";") -notcontains $dir) { $env:Path = "$env:Path;$dir" }

if (-not $Code) { return }
# init spends the code only in a repository's top folder (or an empty one with -New), else prints where to run it
$initArgs = @("init", "--code", $Code)
if ($New) { $initArgs += "--new" }
if ($Here) { $initArgs += "--here" }
if ($Api) { $initArgs += @("--api", $Api) }
& $exe @initArgs
