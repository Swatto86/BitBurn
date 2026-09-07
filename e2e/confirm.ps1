#Requires -Version 5.1
[CmdletBinding()]
param([Parameter(Mandatory)][int]$DriverPid)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$deadline = [DateTime]::UtcNow.AddSeconds(15)
do {
    $windows = [System.Windows.Automation.AutomationElement]::RootElement.FindAll([System.Windows.Automation.TreeScope]::Children, [System.Windows.Automation.Condition]::TrueCondition)
    foreach ($window in $windows) {
        if ($window.Current.Name -like '*WARNING*') {
            $owner = $window.Current.ProcessId
            for ($depth = 0; $depth -lt 8 -and $owner -gt 1 -and $owner -ne $DriverPid; $depth++) {
                $owner = (Get-CimInstance Win32_Process -Filter "ProcessId=$owner").ParentProcessId
            }
            if ($owner -ne $DriverPid) { continue }
            $condition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, 'Yes')
            $button = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
            if ($null -ne $button) {
                $pattern = $button.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
                $pattern.Invoke()
                exit 0
            }
        }
    }
    Start-Sleep -Milliseconds 100
} while ([DateTime]::UtcNow -lt $deadline)
throw 'BitBurn confirmation dialog not found'
