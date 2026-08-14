# Ouvre un shell dans l'environnement ROS 2 Jazzy + Rust, avec le depot monte.
#
# Usage :
#   .\scripts\ros2-shell.ps1              # shell interactif
#   .\scripts\ros2-shell.ps1 colcon build # commande unique

param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$Command = @('bash')
)

$ErrorActionPreference = 'Stop'

$repository = Split-Path -Parent $PSScriptRoot
$image = 'roboto-ros2:jazzy'

if (-not (docker image inspect $image 2>$null)) {
    Write-Host "Image $image absente, construction..." -ForegroundColor Yellow
    docker build -t $image (Join-Path $repository 'docker/ros2')
}

# --network host pour que DDS decouvre les noeuds lances hors du conteneur.
docker run --rm -it `
    --network host `
    -v "${repository}:/workspace" `
    -w /workspace/ros2 `
    $image @Command
