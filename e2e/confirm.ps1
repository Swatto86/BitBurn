#Requires -Version 5.1
[CmdletBinding()]
param([Parameter(Mandatory)][int]$DriverPid)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class NativeDialog {
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr dialog, int id);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr window, StringBuilder text, int count);
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
            # Standard IDYES button in this already-verified test-owned dialog.
            $handle = [NativeDialog]::GetDlgItem([IntPtr]$window.Current.NativeWindowHandle, 6)
            $label = New-Object System.Text.StringBuilder 64
            if ($handle -ne [IntPtr]::Zero) {
                [void][NativeDialog]::GetWindowText($handle, $label, 64)
                if ($label.ToString().Replace('&', '') -eq 'Yes') {
                    $result = [UIntPtr]::Zero
                    if ([NativeDialog]::SendMessageTimeout($handle, 0x00F5, [IntPtr]::Zero, [IntPtr]::Zero, 2, 5000, [ref]$result) -eq [IntPtr]::Zero) { throw 'Native Yes button did not respond' }
                    exit 0
                }
            }
        }
    }
    Start-Sleep -Milliseconds 100
} while ([DateTime]::UtcNow -lt $deadline)
throw 'BitBurn confirmation dialog not found'
