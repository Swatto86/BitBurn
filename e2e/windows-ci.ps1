#Requires -Version 5.1
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true') { throw 'This setup is only for disposable GitHub runners' }
$env:BITBURN_E2E_WEBVIEW_PROFILE = Join-Path $env:RUNNER_TEMP ([guid]::NewGuid().ToString())
$base = 'HKLM:\SOFTWARE\Policies\Microsoft\Edge\WebView2'
$values = @{
    AdditionalBrowserArguments = '--remote-debugging-port=0'
    UserDataFolder = $env:BITBURN_E2E_WEBVIEW_PROFILE
}
$created = @()
try {
    foreach ($name in $values.Keys) {
        $key = Join-Path $base $name
        New-Item -Path $key -Force | Out-Null
        if ($null -ne (Get-Item $key).GetValue('BitBurn.exe')) { throw 'Existing BitBurn policy must not be overwritten' }
        New-ItemProperty -Path $key -Name 'BitBurn.exe' -Value $values[$name] -PropertyType String | Out-Null
        $created += $key
    }
    npm run test:e2e
    $testExit = $LASTEXITCODE
} finally {
    foreach ($key in $created) { Remove-ItemProperty -Path $key -Name 'BitBurn.exe' }
}
exit $testExit
