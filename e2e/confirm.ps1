#Requires -Version 5.1
[CmdletBinding()]
param([Parameter(Mandatory)][int]$DriverPid)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class NativeDialog {
    [DllImport("user32.dll")] public static extern IntPtr SendMessageTimeout(IntPtr window, uint message, IntPtr wParam, IntPtr lParam, uint flags, uint timeout, out UIntPtr result);
}
'@
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
            $buttons = $window.FindAll([System.Windows.Automation.TreeScope]::Descendants, $condition)
            foreach ($button in $buttons) {
                $pattern = $null
                if ($button.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
                    $pattern.Invoke()
                    exit 0
                }

            }
            # rfd uses TaskDialogIndirect with the standard IDYES (6).
            # Task-dialog buttons can have no individual HWND or InvokePattern.
            if ($buttons.Count -gt 0 -and $window.Current.NativeWindowHandle -ne 0) {
                $result = [UIntPtr]::Zero
                if ([NativeDialog]::SendMessageTimeout([IntPtr]$window.Current.NativeWindowHandle, 1126, [IntPtr]6, [IntPtr]::Zero, 2, 5000, [ref]$result) -eq [IntPtr]::Zero) { throw 'Task-dialog Yes button did not respond' }
                exit 0
            }
        }
    }
    Start-Sleep -Milliseconds 100
} while ([DateTime]::UtcNow -lt $deadline)
throw 'BitBurn confirmation dialog not found'
