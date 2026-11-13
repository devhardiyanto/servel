<#
.SYNOPSIS
  servel — Servel CLI for WSL containers (wslc). Installed by the Windows installer.

.DESCRIPTION
  Mengelola service yang sama dengan aplikasi Servel (definisi dari services.json bawaan
  aplikasi) lewat wslc.exe. Container yang dibuat CLI dicatat di state yang sama dengan
  aplikasi (%APPDATA%\com.devhardiyanto.servel\wslc-state.json), jadi aplikasi dan CLI
  bisa dipakai bergantian.

  Kompatibel dengan Windows PowerShell 5.1 dan PowerShell 7+.

.EXAMPLE
  servel preflight               # check prerequisites; changes nothing
  servel up                      # start the services selected in the app (or all if none are selected)
  servel up mysql postgres       # only some
  servel down                    # stop every servel_* container
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
$ControlPath  = Join-Path $AppConfigDir 'control.json'

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
        throw 'wslc not found. Run: wsl --update (needs WSL >= 3.0.1), then open a new terminal.'
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
    if (-not $r.Ok) { throw "wslc container list failed: $($r.Out)" }
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
    if (-not $ServicesJson) { throw "services.json not found next to the CLI ($PSScriptRoot). Reinstall Servel." }
    return @(Get-Content $ServicesJson -Raw -Encoding UTF8 | ConvertFrom-Json)
}

function Resolve-Def([object[]]$Defs, [string]$Id) {
    $d = $Defs | Where-Object { $_.id -eq $Id } | Select-Object -First 1
    if (-not $d) { throw "unknown service '$Id'. Choices: $(($Defs | ForEach-Object { $_.id }) -join ', ')" }
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
        } catch { Write-Warn "could not read wslc-state.json; ignored." }
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
                Write-Step "using the services selected in the app: $($sel -join ', ')"
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
        Write-Warn 'this shell is elevated (Administrator). WSL containers sessions are separate per elevation, so containers from a normal shell or the Servel app are not visible here.'
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
                Write-Step "$name already exists (created outside Servel); using it as it is"
            }
            $upToDate = (-not $recorded) -or ($recorded -eq $desired)

            if ($existing -and $upToDate) {
                if ($existing.Running) { Write-Step "$name is already running"; continue }
                Write-Step "start $name"
                $r = Invoke-Wslc @('start', $name)
                if (-not $r.Ok) { throw $r.Out }
                continue
            }

            if ($existing) {
                Write-Step "the definition of $name changed; recreating the container (volume data is kept)"
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
                        if ($vc.Ok) { Write-Step "volume $vol created" }
                        $volumes += $vol
                    }
                }
            }

            # Pull terpisah supaya progress terlihat; lewati bila image sudah ada.
            $img = [string]$d.image
            $has = Invoke-Wslc @('image', 'inspect', $img) 1
            if (-not $has.Ok) {
                Write-Step "pull $img"
                if ((Invoke-WslcLive @('pull', $img)) -ne 0) { throw "pull $img failed" }
            }

            Write-Step "run $name ($img)"
            $r = Invoke-Wslc $runArgs
            if (-not $r.Ok) { throw $r.Out }
            $state[$name] = $desired
        } catch {
            $failed += $id
            Write-Fail "$id failed: $($_.Exception.Message)"
        } finally {
            Write-State $state
        }
    }

    if ($failed.Count -gt 0) {
        Write-Warn "failed: $($failed -join ', '). Other services keep running; fix the problem, then run 'servel up' again."
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
        if (-not $r.Ok) { Write-Fail "stop $n failed: $($r.Out)" }
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
    if (-not $id) { throw 'usage: servel logs <service> [-f] [-n 100]' }
    $name = (Resolve-Def (Get-Defs) $id).containerName
    $a = @('logs', '--tail', $tail)
    if ($follow) { $a += '-f' }
    exit (Invoke-WslcLive ($a + @($name)))
}

function Invoke-Shell([string[]]$ArgsIn) {
    if (-not $ArgsIn -or $ArgsIn.Count -lt 1) { throw 'usage: servel shell <service> [command...]' }
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
    Write-Warn 'container cp failed; falling back to a stream over stdin.'
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
    if ($p.ExitCode -ne 0) { throw "streaming the file to $Name failed (exit $($p.ExitCode))." }
}

function Invoke-Restore([string[]]$ArgsIn) {
    if (-not $ArgsIn -or $ArgsIn.Count -lt 2) { throw 'usage: servel restore <mysql|postgres> <file.sql | folder of .sql files>' }
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
    if (-not $cmds.ContainsKey($id)) { throw "restore works only for: $($cmds.Keys -join ', ')" }
    if (-not (Test-Path $target)) { throw "not found: $target" }
    $files = if ((Get-Item $target).PSIsContainer) { @(Get-ChildItem $target -Filter *.sql -File | Sort-Object Name) } else { @(Get-Item $target) }
    if ($files.Count -eq 0) { throw "no .sql files in $target" }

    Invoke-Up @($id)
    $name = (Resolve-Def (Get-Defs) $id).containerName

    Write-Step "waiting for $name to be ready (up to 120 s)..."
    $ok = $false
    for ($i = 0; $i -lt 60; $i++) {
        if ((Invoke-Wslc @('exec', $name, 'sh', '-c', $ready[$id]) 1).Ok) { $ok = $true; break }
        Start-Sleep -Seconds 2
    }
    if (-not $ok) { throw "$name was not ready within 120 seconds. Check: servel logs $id" }

    foreach ($f in $files) {
        Write-Step "restore $($f.Name) -> $name"
        Send-File $f.FullName $name
        if ((Invoke-WslcLive @('exec', $name, 'sh', '-c', $cmds[$id])) -ne 0) {
            Write-Fail "restore $($f.Name) failed (exit $LASTEXITCODE)."
            exit 1
        }
        Write-Step "finished $($f.Name)"
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
    if (-not (Get-Command mise -ErrorAction SilentlyContinue)) { throw 'mise not found on PATH.' }
    $global = $false
    $pos = @()
    foreach ($a in $ArgsIn) {
        if ($a -in '-g', '--global') { $global = $true } else { $pos += $a }
    }
    if ($pos.Count -ne 2) { throw 'usage: servel use <php|node|tool> <version> [--global]' }
    $tool = $pos[0]; $version = $pos[1]
    if ($tool -notmatch '^[a-z0-9][a-z0-9_-]*$') { throw "tool '$tool' is not valid" }
    if ($version -notmatch '^\d+(\.\d+){0,2}$') { throw "version '$version' is not valid (examples: 8.3, 22, 22.23.3)" }
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
    if ($LASTEXITCODE -ne 0) { throw "mise ls --current failed (exit $LASTEXITCODE). Run 'mise ls --current' for details (for example the config is not trusted yet)." }
    try { $cur = $json | ConvertFrom-Json } catch { throw "could not read the mise output: $($_.Exception.Message)" }
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
    if (-not (Test-Path $ConfigPath)) { Write-Step 'no projects yet (the app has not created its config).'; return }
    $cfg = Get-Content $ConfigPath -Raw -Encoding UTF8 | ConvertFrom-Json
    $projects = @()
    if ($cfg.PSObject.Properties.Name -contains 'projects') { $projects = @($cfg.projects) }
    if ($projects.Count -eq 0) { Write-Step 'no projects yet. Add one in Servel → Settings → Projects.'; return }
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
            URL     = if (($_.PSObject.Properties.Name -contains 'port') -and ([int]$_.port -gt 0)) { "http://localhost:$($_.port)" } else { '' }
        }
    } | Format-Table -AutoSize | Out-String -Width 200 | Write-Host
}

# Kirim satu perintah ke aplikasi Servel yang sedang berjalan (kanal kontrol
# lokal, lihat src-tauri/src/control.rs). Mengembalikan objek { ok, message,
# projects }, atau $null bila aplikasi tidak berjalan.
function Send-Control([string]$Request) {
    if (-not (Test-Path $ControlPath)) { return $null }
    try { $info = Get-Content $ControlPath -Raw -Encoding UTF8 | ConvertFrom-Json } catch { return $null }
    $client = New-Object System.Net.Sockets.TcpClient
    try {
        # control.json bisa tertinggal dari sesi yang mati paksa: gagal konek = aplikasi tidak berjalan.
        try { $client.Connect([System.Net.IPAddress]::Loopback, [int]$info.port) } catch { return $null }
        $stream = $client.GetStream()
        $stream.ReadTimeout = 120000
        $bytes = [System.Text.Encoding]::UTF8.GetBytes("$($info.token) $Request`n")
        $stream.Write($bytes, 0, $bytes.Length)
        $reader = New-Object System.IO.StreamReader($stream, [System.Text.Encoding]::UTF8)
        $line = $reader.ReadLine()
        if (-not $line) { return $null }
        return ($line | ConvertFrom-Json)
    } finally {
        $client.Close()
    }
}

function Show-ProjectStates($Projects) {
    @($Projects) | ForEach-Object {
        [pscustomobject]@{
            PROJECT = $_.name
            STATE   = if ($_.state -eq 'exited' -and $null -ne $_.exitCode) { "exited ($($_.exitCode))" } else { $_.state }
            URL     = if ([int]$_.port -gt 0) { "http://localhost:$($_.port)" } else { '' }
        }
    } | Format-Table -AutoSize | Out-String -Width 200 | Write-Host
}

# servel start|stop|restart <project> dan servel ps
function Invoke-ProjectControl([string]$Action, [string[]]$ArgsIn) {
    $name = (@($ArgsIn) -join ' ').Trim()
    if ($Action -ne 'status' -and -not $name) { throw "give the project name: servel $Action <project>" }
    $res = Send-Control ("$Action $name".Trim())
    if ($null -eq $res) {
        throw 'the Servel app is not running. Open Servel first; project dev servers are run by the app.'
    }
    if (-not $res.ok) { throw $res.message }
    if ($res.message) { Write-Step $res.message }
    if (@($res.projects).Count -gt 0) { Show-ProjectStates $res.projects }
    elseif ($Action -eq 'status') { Write-Step 'no project has a dev server command yet.' }
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
            if ($busy) { Write-Warn "port $hp ($($d.id)) is in use (expected if that service is already running)." }
        }
    }
    if (Get-Command docker -ErrorAction SilentlyContinue) {
        Write-Warn 'docker.exe is still on PATH. Servel (Auto) will use Docker; choose "WSL containers" in Settings if needed.'
    }
}

function Show-Help {
    @'
servel — manage Servel services through WSL containers (wslc)

  servel preflight                 check wslc, WSL version, elevation, ports
  servel list                      list the available services
  servel up [service...]           start (default: the last selection in the app)
  servel down [service...]         stop (default: every servel_*)
  servel status                    status of every service
  servel logs <service> [-f] [-n N]
  servel shell <service> [cmd...]  shell or command inside the container
  servel restore <mysql|postgres> <file.sql | folder>
  servel use <php|node> <version> [--global]
                                   change the version through mise in this folder (or globally)
  servel projects                  list the projects from the Servel app
  servel start <project>           start the project's dev server (the Servel app must be open)
  servel stop <project>            stop the project's dev server
  servel restart <project>         stop, then start again
  servel ps                        dev server status of every project

Service definitions are the same as in the Servel app; use the app and the CLI interchangeably.
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
        'start'     { Invoke-ProjectControl 'start' $Rest; break }
        'stop'      { Invoke-ProjectControl 'stop' $Rest; break }
        'restart'   { Invoke-ProjectControl 'restart' $Rest; break }
        'ps'        { Invoke-ProjectControl 'status' $Rest; break }
        default     { Write-Fail "unknown command: $Command"; Show-Help; exit 2 }
    }
} catch {
    Write-Fail $_.Exception.Message
    exit 1
}
