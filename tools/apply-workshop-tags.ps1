param(
    [Parameter(Mandatory = $true)]
    [UInt64] $WorkshopId,

    [Parameter(Mandatory = $true)]
    [string] $StationeersDir,

    [Parameter(Mandatory = $true)]
    [string] $TagsJson
)

$ErrorActionPreference = "Stop"
$Tags = @(
    (ConvertFrom-Json -InputObject $TagsJson) |
        ForEach-Object { [string] $_ } |
        ForEach-Object { $_.Trim() } |
        Where-Object { $_ }
)
if ($Tags.Count -eq 0) {
    throw "At least one Workshop tag is required"
}

$managedAssembly = Join-Path $StationeersDir "rocketstation_Data\Managed\Facepunch.Steamworks.Win64.dll"
$nativeDirectory = Join-Path $StationeersDir "rocketstation_Data\Plugins\x86_64"

if (-not (Test-Path -LiteralPath $managedAssembly)) {
    throw "Facepunch.Steamworks.Win64.dll was not found under STATIONEERS_DIR: $managedAssembly"
}
if (-not (Test-Path -LiteralPath (Join-Path $nativeDirectory "steam_api64.dll"))) {
    throw "steam_api64.dll was not found under STATIONEERS_DIR: $nativeDirectory"
}

$env:PATH = "$nativeDirectory;$env:PATH"
Add-Type -Path $managedAssembly

$steamInitialized = $false

function Wait-SteamTask {
    param(
        [Parameter(Mandatory = $true)]
        [System.Threading.Tasks.Task] $Task,

        [Parameter(Mandatory = $true)]
        [string] $Description
    )

    $deadline = [DateTime]::UtcNow.AddSeconds(60)
    while (-not $Task.IsCompleted -and [DateTime]::UtcNow -lt $deadline) {
        [Steamworks.SteamClient]::RunCallbacks()
        Start-Sleep -Milliseconds 50
    }

    if (-not $Task.IsCompleted) {
        throw "Steam Workshop $Description timed out"
    }
    if ($Task.IsFaulted) {
        throw $Task.Exception.InnerException
    }
}

try {
    [Steamworks.SteamClient]::Init([UInt32] 544550, $false)
    $steamInitialized = $true

    if (-not [Steamworks.SteamClient]::IsValid) {
        throw "Steamworks could not initialize. Start Steam and make sure you own Stationeers."
    }
    Write-Output "Steamworks initialized for $([Steamworks.SteamClient]::Name) ($([Steamworks.SteamClient]::SteamId))."

    $publishedFileId = [Steamworks.Data.PublishedFileId]::op_Implicit($WorkshopId)
    $editor = [Steamworks.Ugc.Editor]::new($publishedFileId)
    foreach ($tag in $Tags) {
        $editor = $editor.WithTag($tag)
    }
    Write-Output "Submitting separate Workshop tags: $($Tags -join ', ')"

    $submitTask = $editor.SubmitAsync($null)
    Wait-SteamTask $submitTask "tag update"

    $result = $submitTask.Result
    if (-not $result.Success) {
        throw "Steam Workshop tag update failed: $($result.Result)"
    }

    Write-Output "Applied $($Tags.Count) Workshop tag(s): $($Tags -join ', ')"
}
finally {
    if ($steamInitialized -and [Steamworks.SteamClient]::IsValid) {
        [Steamworks.SteamClient]::Shutdown()
    }
}
