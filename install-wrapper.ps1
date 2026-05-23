param(
    [Parameter(Mandatory)][string]$InstallDir
)

$wrapper = "$env:LOCALAPPDATA\Microsoft\WindowsApps\ssh-manager.bat"
$content = "@echo off`r`npython `"$InstallDir\main.py`" %*`r`n"

# Write as ASCII (no BOM) — .bat files must not have a UTF-8 BOM
[System.IO.File]::WriteAllText($wrapper, $content, [System.Text.Encoding]::ASCII)
Write-Host "Installed: $wrapper"
