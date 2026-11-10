<#
  Tambah / hapus folder CLI Servel di PATH user. Dipanggil installer NSIS
  (post-install: -Action add, pre-uninstall: -Action remove).

  Ditulis langsung ke HKCU\Environment dengan nilai TIDAK di-expand dan tipe
  REG_EXPAND_SZ, supaya entri seperti %USERPROFILE%\... milik user tetap utuh
  (berbeda dengan [Environment]::SetEnvironmentVariable yang meng-expand semuanya).
  Kompatibel Windows PowerShell 5.1.
#>
param(
    [Parameter(Mandatory = $true)][ValidateSet('add', 'remove')][string]$Action,
    [Parameter(Mandatory = $true)][string]$Dir
)
$ErrorActionPreference = 'Stop'

$norm = { param($p) $p.Trim().TrimEnd('\').ToLowerInvariant() }
$target = & $norm $Dir

$key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
if (-not $key) { $key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment') }
try {
    $current = [string]$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
    $parts = @($current -split ';' | Where-Object { $_.Trim() -ne '' })
    $others = @($parts | Where-Object { (& $norm $_) -ne $target })

    if ($Action -eq 'add') {
        if ($others.Count -ne $parts.Count) { Write-Output "PATH sudah berisi $Dir"; return }
        $new = ($parts + $Dir) -join ';'
    } else {
        if ($others.Count -eq $parts.Count) { Write-Output "PATH tidak berisi $Dir"; return }
        $new = $others -join ';'
    }
    $key.SetValue('Path', $new, [Microsoft.Win32.RegistryValueKind]::ExpandString)
    Write-Output "PATH user diperbarui ($Action $Dir)"
} finally {
    $key.Close()
}

# Broadcast WM_SETTINGCHANGE supaya terminal / Explorer yang baru dibuka membaca PATH baru.
# Set lalu hapus variabel dummy: .NET mengirim broadcast untuk keduanya.
[Environment]::SetEnvironmentVariable('SERVEL_PATH_REFRESH', '1', 'User')
[Environment]::SetEnvironmentVariable('SERVEL_PATH_REFRESH', $null, 'User')
