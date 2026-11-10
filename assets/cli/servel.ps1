<#
.SYNOPSIS
  servel — CLI Servel untuk WSL containers (wslc). Dipasang otomatis oleh installer Windows.

.DESCRIPTION
  Mengelola service yang sama dengan aplikasi Servel (definisi dari services.json bawaan
  aplikasi) lewat wslc.exe. Container yang dibuat CLI dicatat di state yang sama dengan
  aplikasi (%APPDATA%\com.devhardiyanto.servel\wslc-state.json), jadi aplikasi dan CLI
  bisa dipakai bergantian.

  Kompatibel dengan Windows PowerShell 5.1 dan PowerShell 7+.

.EXAMPLE
  servel preflight               # cek prasyarat, tidak mengubah apa pun
  servel up                      # start service pilihan di aplikasi (atau semua bila belum ada)
  servel up mysql postgres       # hanya sebagian
  servel down                    # stop semua container servel_*
  servel status
  servel logs mysql -f
  servel shell postgres
  servel restore mysql D:\backup\dump.sql
#>
param(
    [Parameter(Position = 0)]
    [string]$Command = 'help',

    [Parameter(Position = 1, ValueFromRemainingArguments = $true)]
    [string[]]$Rest = @()
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2.0

# ---------------------------------------------------------------------------
# Lokasi
# ---------------------------------------------------------------------------
$AppId        = 'com.devhardiyanto.servel'
$AppConfigDir = Join-Path $env:APPDATA $AppId
$StatePath    = Join-Path $AppConfigDir 'wslc-state.json'
$ConfigPath   = Join-Path $AppConfigDir 'config.json'

# Layout instalasi: <INSTDIR>\cli\servel.ps1 + <INSTDIR>\services\services.json.
# Layout repo (assets\cli + assets\services) identik, jadi satu path cukup.
$ServicesJson = [System.IO.Path]::GetFullPath([System.IO.Path]::Combine($PSScriptRoot, '..', 'services', 'services.json'))
if (-not (Test-Path $ServicesJson)) { $ServicesJson = $null }

# ---------------------------------------------------------------------------
# Output
# ---------------------------------------------------------------------------
function Write-Step([string]$Msg) { Write-Host "[servel] $Msg" -ForegroundColor Cyan }
function Write-Warn([string]$Msg) { Write-Host "[servel] $Msg" -ForegroundColor Yellow }
function Write-Fail([string]$Msg) { Write-Host "[servel] $Msg" -ForegroundColor Red }

# ---------------------------------------------------------------------------
# wslc
# ---------------------------------------------------------------------------
function Assert-Wslc {
    if (-not (Get-Command wslc -ErrorAction SilentlyContinue)) {
        throw 'wslc tidak ditemukan. Jalankan: wsl --update (butuh WSL >= 3.0.1), lalu buka terminal baru.'
    }
}

# Windows PowerShell 5.1 dan pwsh < 7.3 ("Legacy") membuang tanda kutip ganda di
# argumen program native. Di mode itu, escape `"` → `\"` supaya wslc menerima apa adanya.
$LegacyArgPassing = $true
$napVar = Get-Variable -Name PSNativeCommandArgumentPassing -ErrorAction SilentlyContinue
if ($napVar -and $napVar.Value -ne 'Legacy') { $LegacyArgPassing = $false }

function ConvertTo-NativeArgs([string[]]$A) {
    if (-not $LegacyArgPassing) { return ,$A }
    return ,[string[]]@($A | ForEach-Object { $_ -replace '"', '\"' })
}

# Jalankan wslc, tangkap output (stdout+stderr sebagai string). Error transien
# WSLC (sesi dipakai proses lain, mis. aplikasi Servel yang sedang polling)
# di-retry. Return @{ Ok; Code; Out }.
function Invoke-Wslc([string[]]$WslcArgs, [int]$Attempts = 4) {
    for ($i = 1; $i -le $Attempts; $i++) {
        $prev = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        try {
            $native = ConvertTo-NativeArgs $WslcArgs
            $out = (& wslc @native 2>&1 | ForEach-Object { "$_" }) -join "`n"
            $code = $LASTEXITCODE
        } finally {
            $ErrorActionPreference = $prev
        }
        if ($code -eq 0) { return @{ Ok = $true; Code = 0; Out = $out } }
        $transient = $out -match 'ERROR_SHARING_VIOLATION|ERROR_ALREADY_EXISTS'
        if (-not $transient -or $i -eq $Attempts) { return @{ Ok = $false; Code = $code; Out = $out } }
        Start-Sleep -Milliseconds 1500
    }
}

# Jalankan wslc dengan output langsung ke terminal (pull progress, logs -f, shell).
function Invoke-WslcLive([string[]]$WslcArgs) {
    $native = ConvertTo-NativeArgs $WslcArgs
    & wslc @native
    return $LASTEXITCODE
}

# `wslc container list --all --format json` → NDJSON bergaya Docker (terverifikasi WSL 3.0.5).
function Get-Containers {
    $r = Invoke-Wslc @('container', 'list', '--all', '--format', 'json')
    if (-not $r.Ok) { throw "wslc container list gagal: $($r.Out)" }
    $list = @()
    foreach ($line in ($r.Out -split "`n")) {
        $t = $line.Trim()
        if (-not $t) { continue }
        $items = @()
        try { $items = @($t | ConvertFrom-Json) } catch { continue }
        foreach ($o in $items) {
            $name = $null
            foreach ($k in 'Names', 'Name') {
                if ($o.PSObject.Properties.Name -contains $k -and $o.$k) { $name = [string](@($o.$k)[0]); break }
            }
            if (-not $name) { continue }
            $name = $name.TrimStart('/')
            $state = ''
            if ($o.PSObject.Properties.Name -contains 'State') {
                $s = $o.State
                if ($s -is [string]) { $state = $s.ToLowerInvariant() }
                elseif ($s -is [int] -or $s -is [long]) { $state = @{ 1 = 'created'; 2 = 'running'; 3 = 'exited' }[[int]$s] }
            }
            $status = ''
            if ($o.PSObject.Properties.Name -contains 'Status') { $status = [string]$o.Status }
            if (-not $state) { $state = if ($status -match '^(?i)up') { 'running' } else { 'exited' } }
            $list += [pscustomobject]@{ Name = $name; State = $state; Status = $status; Running = ($state -eq 'running') }
        }
    }
    return $list
}

function Get-Container([object[]]$All, [string]$Name) {
    return $All | Where-Object { $_.Name -eq $Name } | Select-Object -First 1
}

# ---------------------------------------------------------------------------
# Definisi service (sumber yang sama dengan aplikasi)
# ---------------------------------------------------------------------------
function Get-Defs {
    if (-not $ServicesJson) { throw "services.json tidak ditemukan di sebelah CLI ($PSScriptRoot). Install ulang Servel." }
    return @(Get-Content $ServicesJson -Raw -Encoding UTF8 | ConvertFrom-Json)
}

function Resolve-Def([object[]]$Defs, [string]$Id) {
    $d = $Defs | Where-Object { $_.id -eq $Id } | Select-Object -First 1
    if (-not $d) { throw "service '$Id' tidak dikenal. Pilihan: $(($Defs | ForEach-Object { $_.id }) -join ', ')" }
    return $d
}

# Pecah string command bergaya shell. HARUS identik dengan `split_command` di
# src-tauri/src/runtime/wslc.rs supaya spec hash CLI = aplikasi.
function Split-Command([string]$InputText) {
    $args_ = New-Object System.Collections.Generic.List[string]
    $cur = New-Object System.Text.StringBuilder
    $inToken = $false
    $quote = [char]0
    $chars = $InputText.ToCharArray()
    $i = 0
    while ($i -lt $chars.Length) {
        $c = $chars[$i]
        $next = if ($i + 1 -lt $chars.Length) { $chars[$i + 1] } else { [char]0 }
        if ($quote -ne [char]0) {
            if ($c -eq $quote) { $quote = [char]0 }
            elseif ($quote -eq [char]"'") { [void]$cur.Append($c) }
            elseif ($c -eq [char]'\' -and ($next -eq [char]'"' -or $next -eq [char]'\')) { [void]$cur.Append($next); $i++ }
            else { [void]$cur.Append($c) }
        } elseif ($c -eq [char]'"' -or $c -eq [char]"'") {
            $quote = $c; $inToken = $true
        } elseif ($c -eq [char]'\') {
            if ($next -ne [char]0) { [void]$cur.Append($next); $i++ }
            $inToken = $true
        } elseif ([char]::IsWhiteSpace($c)) {
            if ($inToken) { $args_.Add($cur.ToString()); [void]$cur.Clear(); $inToken = $false }
        } else {
            [void]$cur.Append($c); $inToken = $true
        }
        $i++
    }
    if ($inToken) { $args_.Add($cur.ToString()) }
    return ,$args_.ToArray()
}

# Argumen `wslc run`. HARUS identik dengan `build_run_args` di wslc.rs.
function Get-RunArgs($Def) {
    $a = New-Object System.Collections.Generic.List[string]
    foreach ($x in @('run', '-d', '--name', $Def.containerName)) { $a.Add($x) }
    foreach ($p in @($Def.ports)) { $a.Add('-p'); $a.Add("127.0.0.1:$($p.host):$($p.container)") }
    if ($Def.PSObject.Properties.Name -contains 'volumes') {
        foreach ($v in @($Def.volumes)) { $a.Add('-v'); $a.Add([string]$v) }
    }
    if ($Def.PSObject.Properties.Name -contains 'environment' -and $Def.environment) {
        # Urutan ordinal (byte), sama dengan BTreeMap di Rust.
        $keys = [string[]]@($Def.environment.PSObject.Properties | ForEach-Object { $_.Name })
        [Array]::Sort($keys, [StringComparer]::Ordinal)
        foreach ($k in $keys) { $a.Add('-e'); $a.Add("$k=$($Def.environment.$k)") }
    }
    $a.Add([string]$Def.image)
    if ($Def.PSObject.Properties.Name -contains 'command' -and $Def.command) {
        foreach ($x in (Split-Command ([string]$Def.command))) { $a.Add($x) }
    }
    return ,$a.ToArray()
}

# Sidik definisi. HARUS identik dengan `spec_hash` di wslc.rs:
# SHA-256 atas setiap argumen UTF-8 diikuti byte 0, ambil 8 byte pertama (hex).
function Get-SpecHash([string[]]$RunArgs) {
    $ms = New-Object System.IO.MemoryStream
    foreach ($x in $RunArgs) {
        $b = [System.Text.Encoding]::UTF8.GetBytes($x)
        $ms.Write($b, 0, $b.Length)
        $ms.WriteByte(0)
    }
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try { $h = $sha.ComputeHash($ms.ToArray()) } finally { $sha.Dispose() }
    return (($h[0..7] | ForEach-Object { $_.ToString('x2') }) -join '')
}

function Get-NamedVolume([string]$Spec) {
    if ($Spec -match '^[A-Za-z]:[\\/]') { return $null }
    $name = $Spec.Split(':')[0].Trim()
    if (-not $name -or $name -match '[/\\.]') { return $null }
    return $name
}

# ---------------------------------------------------------------------------
# State bersama aplikasi
# ---------------------------------------------------------------------------
function Read-State {
    $h = @{}
    if (Test-Path $StatePath) {
        try {
            $o = Get-Content $StatePath -Raw -Encoding UTF8 | ConvertFrom-Json
            foreach ($p in $o.PSObject.Properties) { $h[$p.Name] = [string]$p.Value }
        } catch { Write-Warn "wslc-state.json tidak terbaca, diabaikan." }
    }
    return $h
}

function Write-State([hashtable]$State) {
    New-Item -ItemType Directory -Force -Path $AppConfigDir | Out-Null
    $ordered = [ordered]@{}
    foreach ($k in ($State.Keys | Sort-Object)) { $ordered[$k] = $State[$k] }
    $json = ($ordered | ConvertTo-Json)
    if (-not $json) { $json = '{}' }
    $tmp = "$StatePath.tmp"
    [System.IO.File]::WriteAllText($tmp, $json, (New-Object System.Text.UTF8Encoding($false)))
    Move-Item -Force $tmp $StatePath
}

# Pilihan service: argumen > pilihan terakhir di aplikasi > semua.
function Resolve-Targets([object[]]$Defs, [string[]]$Ids) {
    if ($Ids -and $Ids.Count -gt 0) { return @($Ids | ForEach-Object { (Resolve-Def $Defs $_).id }) }
    if (Test-Path $ConfigPath) {
        try {
            $cfg = Get-Content $ConfigPath -Raw -Encoding UTF8 | ConvertFrom-Json
            $sel = @($cfg.selectedServiceIds)
            if ($sel.Count -gt 0) {
                Write-Step "memakai pilihan service dari aplikasi: $($sel -join ', ')"
                return $sel
            }
        } catch { }
    }
    return @($Defs | ForEach-Object { $_.id })
}

function Test-Elevated {
    try {
        $id = [Security.Principal.WindowsIdentity]::GetCurrent()
        return ([Security.Principal.WindowsPrincipal]$id).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    } catch { return $false }
}

function Warn-IfElevated {
    if (Test-Elevated) {
        Write-Warn 'shell ini elevated (Administrator). Sesi WSL containers terpisah per elevation — container dari shell biasa / aplikasi Servel tidak terlihat di sini.'
    }
}

# ---------------------------------------------------------------------------
# Perintah
# ---------------------------------------------------------------------------
function Invoke-Up([string[]]$Ids) {
    $defs = Get-Defs
    $targets = Resolve-Targets $defs $Ids
    $all = Get-Containers
    $state = Read-State
    $volumes = $null
    $failed = @()

    foreach ($id in $targets) {
        try {
            $d = Resolve-Def $defs $id
            $name = [string]$d.containerName
            $runArgs = Get-RunArgs $d
            $desired = Get-SpecHash $runArgs
            $existing = Get-Container $all $name
            $recorded = $state[$name]

            if ($existing -and -not $recorded) {
                Write-Step "$name sudah ada (dibuat di luar Servel) — dipakai apa adanya"
            }
            $upToDate = (-not $recorded) -or ($recorded -eq $desired)

            if ($existing -and $upToDate) {
                if ($existing.Running) { Write-Step "$name sudah berjalan"; continue }
                Write-Step "start $name"
                $r = Invoke-Wslc @('start', $name)
                if (-not $r.Ok) { throw $r.Out }
                continue
            }

            if ($existing) {
                Write-Step "definisi $name berubah — membuat ulang container (data volume tetap)"
                $r = Invoke-Wslc @('remove', '-f', $name)
                if (-not $r.Ok) { throw $r.Out }
                $state.Remove($name)
            }

            if ($null -eq $volumes) {
                $vr = Invoke-Wslc @('volume', 'list', '--format', 'json')
                $volumes = @()
                if ($vr.Ok) {
                    foreach ($line in ($vr.Out -split "`n")) {
                        try { foreach ($o in @($line.Trim() | ConvertFrom-Json)) { if ($o.Name) { $volumes += [string]$o.Name } } } catch { }
                    }
                }
            }
            if ($d.PSObject.Properties.Name -contains 'volumes') {
                foreach ($v in @($d.volumes)) {
                    $vol = Get-NamedVolume ([string]$v)
                    if ($vol -and ($volumes -notcontains $vol)) {
                        $vc = Invoke-Wslc @('volume', 'create', $vol)
                        if ($vc.Ok) { Write-Step "volume $vol dibuat" }
                        $volumes += $vol
                    }
                }
            }

            # Pull terpisah supaya progress terlihat; lewati bila image sudah ada.
            $img = [string]$d.image
            $has = Invoke-Wslc @('image', 'inspect', $img) 1
            if (-not $has.Ok) {
                Write-Step "pull $img"
                if ((Invoke-WslcLive @('pull', $img)) -ne 0) { throw "pull $img gagal" }
            }

            Write-Step "run $name ($img)"
            $r = Invoke-Wslc $runArgs
            if (-not $r.Ok) { throw $r.Out }
            $state[$name] = $desired
        } catch {
            $failed += $id
            Write-Fail "$id gagal: $($_.Exception.Message)"
        } finally {
            Write-State $state
        }
    }

    if ($failed.Count -gt 0) {
        Write-Warn "gagal: $($failed -join ', '). Service lain tetap berjalan; perbaiki lalu jalankan 'servel up' lagi."
        exit 1
    }
}

function Invoke-Down([string[]]$Ids) {
    $defs = Get-Defs
    $all = Get-Containers
    if ($Ids -and $Ids.Count -gt 0) {
        $names = @($Ids | ForEach-Object { (Resolve-Def $defs $_).containerName })
    } else {
        # Urutan terbalik dari services.json, sama seperti aplikasi.
        $order = @($defs | ForEach-Object { $_.containerName })
        $names = @($all | Where-Object { $_.Name -like 'servel_*' } | ForEach-Object { $_.Name } |
            Sort-Object { -1 * [array]::IndexOf($order, $_) })
    }
    foreach ($n in $names) {
        $c = Get-Container $all $n
        if (-not $c -or -not $c.Running) { continue }
        Write-Step "stop $n"
        $r = Invoke-Wslc @('stop', '-t', '30', $n)
        if (-not $r.Ok) { Write-Fail "stop $n gagal: $($r.Out)" }
    }
}

function Invoke-Status {
    $defs = Get-Defs
    $all = Get-Containers
    $state = Read-State
    $rows = foreach ($d in $defs) {
        $c = Get-Container $all $d.containerName
        $ports = (@($d.ports) | ForEach-Object { ":$($_.host)" }) -join ' '
        [pscustomobject]@{
            SERVICE = $d.id
            STATE   = if ($c) { $c.State } else { '-' }
            STATUS  = if ($c) { $c.Status } else { 'not created' }
            PORTS   = $ports
            OWNER   = if (-not $c) { '' } elseif ($state.ContainsKey($d.containerName)) { 'servel' } else { 'adopted' }
        }
    }
    $rows | Format-Table -AutoSize | Out-String -Width 200 | Write-Host
}

function Invoke-List {
    Get-Defs | ForEach-Object {
        [pscustomobject]@{
            SERVICE = $_.id
            IMAGE   = $_.image
            PORTS   = (@($_.ports) | ForEach-Object { "$($_.host)->$($_.container)" }) -join ', '
        }
    } | Format-Table -AutoSize | Out-String -Width 200 | Write-Host
}

function Invoke-Logs([string[]]$ArgsIn) {
    $id = $null; $follow = $false; $tail = '100'
    for ($i = 0; $i -lt $ArgsIn.Count; $i++) {
        switch -Regex ($ArgsIn[$i]) {
            '^(-f|--follow)$' { $follow = $true; continue }
            '^(-n|--tail)$'   { $tail = $ArgsIn[$i + 1]; $i++; continue }
            default           { if (-not $id) { $id = $ArgsIn[$i] } }
        }
    }
    if (-not $id) { throw 'pakai: servel logs <service> [-f] [-n 100]' }
    $name = (Resolve-Def (Get-Defs) $id).containerName
    $a = @('logs', '--tail', $tail)
    if ($follow) { $a += '-f' }
    exit (Invoke-WslcLive ($a + @($name)))
}

function Invoke-Shell([string[]]$ArgsIn) {
    if (-not $ArgsIn -or $ArgsIn.Count -lt 1) { throw 'pakai: servel shell <service> [perintah...]' }
    $name = (Resolve-Def (Get-Defs) $ArgsIn[0]).containerName
    $cmd = if ($ArgsIn.Count -gt 1) { $ArgsIn[1..($ArgsIn.Count - 1)] } else { @('sh') }
    exit (Invoke-WslcLive (@('exec', '-it', $name) + $cmd))
}

# Kirim file host → /tmp/restore.sql di container.
# Jalur 1: `container cp` ke DIREKTORI /tmp/ (tujuan file baru ditolak wslc), lalu rename.
# Jalur 2: stream byte mentah via redirect cmd.exe (binary-safe, tanpa batas ukuran).
function Send-File([string]$Path, [string]$Name) {
    $leaf = Split-Path $Path -Leaf
    Push-Location (Split-Path $Path)
    try { $r = Invoke-Wslc @('container', 'cp', $leaf, "${Name}:/tmp/") 1 } finally { Pop-Location }
    if ($r.Ok) {
        $mv = Invoke-Wslc @('exec', '-e', "SRC=$leaf", $Name, 'sh', '-c', 'mv -f "/tmp/$SRC" /tmp/restore.sql') 1
        if ($mv.Ok) { return }
    }
    Write-Warn 'container cp gagal; fallback ke stream via stdin.'
    # ProcessStartInfo.Arguments (string mentah) ada di .NET Framework maupun .NET 8,
    # jadi tidak bergantung pada aturan quoting PowerShell.
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = (Get-Command wslc).Source
    $psi.Arguments = "exec -i $Name sh -c `"cat > /tmp/restore.sql`""
    $psi.RedirectStandardInput = $true
    $psi.UseShellExecute = $false
    $p = [System.Diagnostics.Process]::Start($psi)
    $fs = [System.IO.File]::OpenRead($Path)
    try { $fs.CopyTo($p.StandardInput.BaseStream) } finally { $fs.Dispose(); $p.StandardInput.Close() }
    $p.WaitForExit()
    if ($p.ExitCode -ne 0) { throw "stream file ke $Name gagal (exit $($p.ExitCode))." }
}

function Invoke-Restore([string[]]$ArgsIn) {
    if (-not $ArgsIn -or $ArgsIn.Count -lt 2) { throw 'pakai: servel restore <mysql|postgres> <file.sql | folder berisi .sql>' }
    $id = $ArgsIn[0]
    $target = $ArgsIn[1]
    $cmds = @{
        mysql    = 'MYSQL_PWD="${MYSQL_ROOT_PASSWORD:-}" mysql -uroot < /tmp/restore.sql && rm -f /tmp/restore.sql'
        postgres = 'psql -v ON_ERROR_STOP=1 -U "${POSTGRES_USER:-postgres}" -d postgres -f /tmp/restore.sql && rm -f /tmp/restore.sql'
    }
    $ready = @{
        mysql    = 'MYSQL_PWD="${MYSQL_ROOT_PASSWORD:-}" mysqladmin ping -h 127.0.0.1 -uroot --silent'
        postgres = 'pg_isready -h 127.0.0.1 -U "${POSTGRES_USER:-postgres}" -q'
    }
    if (-not $cmds.ContainsKey($id)) { throw "restore hanya untuk: $($cmds.Keys -join ', ')" }
    if (-not (Test-Path $target)) { throw "tidak ada: $target" }
    $files = if ((Get-Item $target).PSIsContainer) { @(Get-ChildItem $target -Filter *.sql -File | Sort-Object Name) } else { @(Get-Item $target) }
    if ($files.Count -eq 0) { throw "tidak ada file .sql di $target" }

    Invoke-Up @($id)
    $name = (Resolve-Def (Get-Defs) $id).containerName

    Write-Step "menunggu $name siap (maks 120 dtk)..."
    $ok = $false
    for ($i = 0; $i -lt 60; $i++) {
        if ((Invoke-Wslc @('exec', $name, 'sh', '-c', $ready[$id]) 1).Ok) { $ok = $true; break }
        Start-Sleep -Seconds 2
    }
    if (-not $ok) { throw "$name tidak siap dalam 120 detik. Cek: servel logs $id" }

    foreach ($f in $files) {
        Write-Step "restore $($f.Name) -> $name"
        Send-File $f.FullName $name
        if ((Invoke-WslcLive @('exec', $name, 'sh', '-c', $cmds[$id])) -ne 0) {
            Write-Fail "restore $($f.Name) gagal (exit $LASTEXITCODE)."
            exit 1
        }
        Write-Step "selesai $($f.Name)"
    }
}

# ---------------------------------------------------------------------------
# mise (v1.9) — logika sama dengan src-tauri/src/mise.rs
# ---------------------------------------------------------------------------
function Get-MiseGlobalRoots {
    $roots = @()
    foreach ($v in @($env:MISE_GLOBAL_CONFIG_FILE, $env:MISE_CONFIG_DIR)) { if ($v) { $roots += $v } }
    if ($env:XDG_CONFIG_HOME) { $roots += (Join-Path $env:XDG_CONFIG_HOME 'mise') }
    $home_ = if ($env:USERPROFILE) { $env:USERPROFILE } else { $HOME }
    if ($home_) {
        $roots += (Join-Path (Join-Path $home_ '.config') 'mise')
        $roots += (Join-Path $home_ '.tool-versions')
    }
    return @($roots | ForEach-Object { $_.Replace('\', '/').TrimEnd('/').ToLowerInvariant() })
}

function Test-MiseGlobalSource([string]$Path) {
    if (-not $Path) { return $false }
    $p = $Path.Replace('\', '/').TrimEnd('/').ToLowerInvariant()
    foreach ($r in (Get-MiseGlobalRoots)) {
        if ($p -eq $r -or $p.StartsWith("$r/")) { return $true }
    }
    return $false
}

function Invoke-Use([string[]]$ArgsIn) {
    if (-not (Get-Command mise -ErrorAction SilentlyContinue)) { throw 'mise tidak ditemukan di PATH.' }
    $global = $false
    $pos = @()
    foreach ($a in $ArgsIn) {
        if ($a -in '-g', '--global') { $global = $true } else { $pos += $a }
    }
    if ($pos.Count -ne 2) { throw 'pakai: servel use <php|node|tool> <versi> [--global]' }
    $tool = $pos[0]; $version = $pos[1]
    if ($tool -notmatch '^[a-z0-9][a-z0-9_-]*$') { throw "tool '$tool' tidak valid" }
    if ($version -notmatch '^\d+(\.\d+){0,2}$') { throw "versi '$version' tidak valid (contoh: 8.3, 22, 22.23.3)" }
    $spec = "$tool@$version"

    if ($global) {
        Write-Step "mise use -g $spec"
        exit (Invoke-NativeLive 'mise' @('use', '-g', $spec))
    }

    # Tulis ke config yang memang berlaku untuk folder ini (bisa di folder induk).
    $target = $null
    $cur = $null
    # EAP Continue: di PowerShell 5.1 baris stderr mise (mis. WARN update) bisa
    # menjadi error terminating walau di-redirect, lalu target jatuh diam-diam
    # ke folder saat ini.
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try { $json = (& mise ls --current --json 2>$null | Out-String) } finally { $ErrorActionPreference = $prevEap }
    if ($LASTEXITCODE -ne 0) { throw "mise ls --current gagal (exit $LASTEXITCODE). Jalankan 'mise ls --current' untuk detailnya (mis. config belum di-trust)." }
    try { $cur = $json | ConvertFrom-Json } catch { throw "output mise tidak bisa dibaca: $($_.Exception.Message)" }
    if ($cur) {
        $projectSources = @()
        foreach ($prop in $cur.PSObject.Properties) {
            foreach ($e in @($prop.Value)) {
                $src = $null
                if ($e.PSObject.Properties.Name -contains 'source' -and $e.source) { $src = [string]$e.source.path }
                if ($src -and -not (Test-MiseGlobalSource $src)) {
                    if ($prop.Name -eq $tool) { $target = $src }
                    $projectSources += $src
                }
            }
        }
        if (-not $target -and $projectSources.Count -gt 0) {
            $target = $projectSources | Sort-Object Length -Descending | Select-Object -First 1
        }
    }
    if (-not $target) { $target = Join-Path (Get-Location) 'mise.toml' }
    Write-Step "mise use --path $target $spec"
    exit (Invoke-NativeLive 'mise' @('use', '--path', $target, $spec))
}

function Invoke-NativeLive([string]$Exe, [string[]]$A) {
    $native = ConvertTo-NativeArgs $A
    & $Exe @native
    return $LASTEXITCODE
}

function Invoke-Projects {
    if (-not (Test-Path $ConfigPath)) { Write-Step 'belum ada project (config aplikasi belum dibuat).'; return }
    $cfg = Get-Content $ConfigPath -Raw -Encoding UTF8 | ConvertFrom-Json
    $projects = @()
    if ($cfg.PSObject.Properties.Name -contains 'projects') { $projects = @($cfg.projects) }
    if ($projects.Count -eq 0) { Write-Step 'belum ada project. Tambahkan di Servel → Settings → PHP & Node.'; return }
    $active = if ($cfg.PSObject.Properties.Name -contains 'activeProjectId') { $cfg.activeProjectId } else { $null }
    $profiles = @{}
    if ($cfg.PSObject.Properties.Name -contains 'profiles') { foreach ($p in @($cfg.profiles)) { $profiles[$p.id] = $p.name } }
    $projects | ForEach-Object {
        $pid_ = if ($_.PSObject.Properties.Name -contains 'profileId') { $_.profileId } else { $null }
        [pscustomobject]@{
            ' '     = if ($_.id -eq $active) { '*' } else { '' }
            PROJECT = $_.name
            PATH    = $_.path
            PROFILE = if ($pid_ -and $profiles.ContainsKey($pid_)) { $profiles[$pid_] } else { '' }
        }
    } | Format-Table -AutoSize | Out-String -Width 200 | Write-Host
}

function Invoke-Preflight {
    Assert-Wslc
    Write-Step 'wsl --version:'
    $env:WSL_UTF8 = '1'
    wsl --version | Select-Object -First 2 | ForEach-Object { Write-Host "  $_" }
    Write-Step 'wslc version:'
    wslc --version | ForEach-Object { Write-Host "  $_" }
    Warn-IfElevated
    Write-Step "services.json: $ServicesJson"
    foreach ($d in Get-Defs) {
        foreach ($p in @($d.ports)) {
            $hp = [int]$p.host
            $busy = $false
            try {
                $l = New-Object System.Net.Sockets.TcpListener([System.Net.IPAddress]::Loopback, $hp)
                $l.Start(); $l.Stop()
            } catch { $busy = $true }
            if ($busy) { Write-Warn "port $hp ($($d.id)) sedang dipakai (wajar bila service itu sudah berjalan)." }
        }
    }
    if (Get-Command docker -ErrorAction SilentlyContinue) {
        Write-Warn 'docker.exe masih ada di PATH. Servel (Auto) akan memakai Docker; pilih "WSL containers" di Settings bila perlu.'
    }
}

function Show-Help {
    @'
servel — kelola service Servel lewat WSL containers (wslc)

  servel preflight                 cek wslc, versi WSL, elevation, port
  servel list                      daftar service yang tersedia
  servel up [service...]           start (default: pilihan terakhir di aplikasi)
  servel down [service...]         stop (default: semua servel_*)
  servel status                    status semua service
  servel logs <service> [-f] [-n N]
  servel shell <service> [cmd...]  shell / perintah di dalam container
  servel restore <mysql|postgres> <file.sql | folder>
  servel use <php|node> <versi> [--global]
                                   ganti versi lewat mise di folder ini (atau global)
  servel projects                  daftar project dari aplikasi Servel

Definisi service sama dengan aplikasi Servel; aplikasi dan CLI bisa dipakai bergantian.
'@ | Write-Host
}

# ---------------------------------------------------------------------------
# Dispatch
# ---------------------------------------------------------------------------
try {
    switch ($Command.ToLowerInvariant()) {
        { $_ -in 'help', '-h', '--help', '/?' } { Show-Help; break }
        'preflight' { Invoke-Preflight; break }
        'list'      { Invoke-List; break }
        'up'        { Assert-Wslc; Warn-IfElevated; Invoke-Up $Rest; break }
        'down'      { Assert-Wslc; Warn-IfElevated; Invoke-Down $Rest; break }
        'status'    { Assert-Wslc; Warn-IfElevated; Invoke-Status; break }
        'logs'      { Assert-Wslc; Invoke-Logs $Rest; break }
        'shell'     { Assert-Wslc; Invoke-Shell $Rest; break }
        'restore'   { Assert-Wslc; Warn-IfElevated; Invoke-Restore $Rest; break }
        'use'       { Invoke-Use $Rest; break }
        'projects'  { Invoke-Projects; break }
        default     { Write-Fail "perintah tidak dikenal: $Command"; Show-Help; exit 2 }
    }
} catch {
    Write-Fail $_.Exception.Message
    exit 1
}
