Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Win32 {
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT r);
    [DllImport("user32.dll")] public static extern void keybd_event(byte bVk, byte bScan, int dwFlags, int dwExtraInfo);
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }
}
"@

# Win+D to show desktop
[Win32]::keybd_event(0x5B, 0, 0, 0)
[Win32]::keybd_event(0x44, 0, 0, 0)
Start-Sleep -Milliseconds 100
[Win32]::keybd_event(0x44, 0, 2, 0)
[Win32]::keybd_event(0x5B, 0, 2, 0)
Start-Sleep -Milliseconds 600

$target = Get-Process msedge -ErrorAction SilentlyContinue |
    Where-Object { $_.MainWindowTitle -match 'Spiral-Coder|127\.0\.0\.1' } |
    Select-Object -First 1

if (-not $target) {
    Start-Process 'http://127.0.0.1:18080/'
    Start-Sleep -Seconds 5
    $target = Get-Process msedge -ErrorAction SilentlyContinue |
        Where-Object { $_.MainWindowTitle -match 'Spiral-Coder|127\.0\.0\.1' } |
        Select-Object -First 1
}

$hwnd = $target.MainWindowHandle
[Win32]::ShowWindow($hwnd, 9) | Out-Null
[Win32]::SetForegroundWindow($hwnd) | Out-Null
Start-Sleep -Milliseconds 800

# F5 to reload
[System.Windows.Forms.SendKeys]::SendWait("{F5}")
Start-Sleep -Seconds 4

# ESC to close any popups
[System.Windows.Forms.SendKeys]::SendWait("{ESC}")
Start-Sleep -Milliseconds 300

$rect = New-Object Win32+RECT
[Win32]::GetWindowRect($hwnd, [ref]$rect) | Out-Null
$x = [Math]::Max(0, $rect.Left)
$y = [Math]::Max(0, $rect.Top)
$w = $rect.Right - $rect.Left
$h = $rect.Bottom - $rect.Top

$bmp = New-Object System.Drawing.Bitmap $w, $h
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($x, $y, 0, 0, [System.Drawing.Size]::new($w, $h))
$bmp.Save('C:\Users\user\spiral-coder\docs\screenshot.png', [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose()
$bmp.Dispose()
Write-Host "saved ${w}x${h}"
