; Kova NSIS installer hook
; Injects %USERPROFILE%\.wslconfig (WSL2 VM limits). Dipakai backend WSL2 Docker Desktop;
; untuk WSL containers (v1.6) efeknya ke VM wslc belum terverifikasi — template tetap aman.
;
; Behaviour:
;   File tidak ada  -> tulis template langsung
;   File sudah ada  -> MessageBox 3-tombol:
;     Yes    = Overwrite (backup .wslconfig.bak, lalu tulis ulang)
;     No     = Append (tambah section [wsl2]/[experimental] kalau belum ada)
;     Cancel = Skip (jangan sentuh file user)
;
; Template sumber: TECHNICAL.md section 10 (verbatim).

!include "FileFunc.nsh"
!include "LogicLib.nsh"
!include "WordFunc.nsh"

; ---------------------------------------------------------------------------
; v3 (Kova): Servel lama terpasang sebagai produk terpisah (nama produk berbeda
; = kunci uninstall dan folder berbeda). Copot diam-diam sebelum memasang Kova.
; /S tidak menampilkan kotak "hapus data", jadi %APPDATA%\com.devhardiyanto.servel
; dan container/volume servel_* tidak disentuh. Uninstaller lama sekaligus
; menghapus folder CLI lamanya dari PATH dan entri Run "Servel".
; ---------------------------------------------------------------------------
; Sisa di folder install Servel lama yang tidak dikenal uninstaller: cadangan
; file hosts dari fitur domain .test (dihapus di v2.2.0). Hanya pola file yang
; dikenal yang dihapus; RMDir tanpa /r, jadi folder berisi file lain dibiarkan.
!macro KOVA_REMOVE_SERVEL_LEFTOVERS DIR
  Delete "${DIR}\hosts-backups\hosts.servel.*.bak"
  RMDir "${DIR}\hosts-backups"
  RMDir "${DIR}"
!macroend

!macro NSIS_HOOK_PREINSTALL
  Push $R0
  Push $R1
  Push $R2
  ReadRegStr $R0 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Servel" "InstallLocation"
  ${If} $R0 != ""
    ; Nilai ditulis dengan tanda kutip: "C:\...\Servel".
    StrCpy $R1 $R0 1
    ${If} $R1 == '"'
      StrCpy $R0 $R0 "" 1
      StrCpy $R0 $R0 -1
    ${EndIf}
    ${If} $R0 != $INSTDIR
    ${AndIf} ${FileExists} "$R0\uninstall.exe"
      DetailPrint "Kova: removing the old Servel install at $R0 (your data stays)..."
      ExecWait '"$R0\uninstall.exe" /S _?=$R0' $R2
      DetailPrint "Kova: old Servel uninstaller finished (exit $R2)."
      ; Dengan _?= uninstaller tidak menghapus dirinya sendiri.
      Delete "$R0\uninstall.exe"
      !insertmacro KOVA_REMOVE_SERVEL_LEFTOVERS "$R0"
    ${EndIf}
  ${EndIf}
  ; Lokasi bawaan: bersihkan juga sisa dari install rc sebelumnya, saat
  ; uninstaller lama sudah jalan tapi foldernya tertinggal.
  ${If} "$LOCALAPPDATA\Servel" != $INSTDIR
  ${AndIfNot} ${FileExists} "$LOCALAPPDATA\Servel\servel.exe"
    !insertmacro KOVA_REMOVE_SERVEL_LEFTOVERS "$LOCALAPPDATA\Servel"
  ${EndIf}
  Pop $R2
  Pop $R1
  Pop $R0
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; CLI `servel` (v1.6): tambahkan $INSTDIR\cli\bin ke PATH user. Logika PATH
  ; ada di install-path.ps1 (aman untuk PATH panjang & entri %VAR%, tidak
  ; terkena batas string 1024 karakter NSIS). Gagal = tidak fatal.
  DetailPrint "Kova: menambahkan perintah 'kova' (dan alias 'servel') ke PATH user..."
  nsExec::ExecToLog 'powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$INSTDIR\cli\install-path.ps1" -Action add -Dir "$INSTDIR\cli\bin"'
  Pop $0

  DetailPrint "Kova: configuring WSL2 (.wslconfig)..."
  Push $0   ; target path
  Push $1   ; file handle / aux
  Push $2   ; whole-file buffer / aux
  Push $R0  ; line read buffer
  Push $R1  ; found-index result

  StrCpy $0 "$PROFILE\.wslconfig"

  ${If} ${FileExists} "$0"
    MessageBox MB_YESNOCANCEL|MB_ICONQUESTION \
      "Kova ingin menulis konfigurasi WSL2 ke:$\r$\n$0$\r$\n$\r$\n\
File sudah ada.$\r$\n$\r$\n\
[Yes]    = Overwrite (file lama dibackup ke .wslconfig.bak)$\r$\n\
[No]     = Append (tambah section [wsl2]/[experimental] kalau belum ada)$\r$\n\
[Cancel] = Skip (jangan ubah)" \
      /SD IDCANCEL IDYES servel_wsl_overwrite IDNO servel_wsl_append

    DetailPrint "Kova: .wslconfig skipped by user."
    Goto servel_wsl_done

    servel_wsl_overwrite:
      DetailPrint "Kova: backing up existing .wslconfig to .wslconfig.bak"
      Delete "$0.bak"
      Rename "$0" "$0.bak"
      Call ServelWriteWslConfigTemplate
      DetailPrint "Kova: .wslconfig overwritten."
      Goto servel_wsl_done

    servel_wsl_append:
      ; Baca seluruh isi file ke $2 lalu cari "[wsl2]"
      ClearErrors
      FileOpen $1 "$0" r
      ${If} ${Errors}
        DetailPrint "Kova: gagal buka .wslconfig untuk append; skipped."
        Goto servel_wsl_done
      ${EndIf}
      StrCpy $2 ""
      servel_wsl_read_loop:
        ClearErrors
        FileRead $1 $R0
        ${If} ${Errors}
          Goto servel_wsl_read_done
        ${EndIf}
        StrCpy $2 "$2$R0"
        Goto servel_wsl_read_loop
      servel_wsl_read_done:
      FileClose $1

      ${WordFind} "$2" "[wsl2]" "E+1{" $R1
      ; ${WordFind} dengan flag "E+1{" mengembalikan "1" kalau error (not found),
      ; selain itu mengembalikan substring sebelum match.
      ${If} $R1 == "1"
        ClearErrors
        FileOpen $1 "$0" a
        ${If} ${Errors}
          DetailPrint "Kova: gagal append .wslconfig; skipped."
          Goto servel_wsl_done
        ${EndIf}
        FileSeek $1 0 END
        FileWrite $1 "$\r$\n"
        Call ServelWriteWslConfigBody
        FileClose $1
        DetailPrint "Kova: .wslconfig appended with [wsl2] section."
      ${Else}
        DetailPrint "Kova: section [wsl2] sudah ada; append dilewati."
      ${EndIf}
      Goto servel_wsl_done
  ${Else}
    DetailPrint "Kova: writing fresh .wslconfig template."
    Call ServelWriteWslConfigTemplate
  ${EndIf}

  servel_wsl_done:
  Pop $R1
  Pop $R0
  Pop $2
  Pop $1
  Pop $0
!macroend

; ---------------------------------------------------------------------------
; Uninstall: JANGAN hapus .wslconfig — preserve user data.
; ---------------------------------------------------------------------------
!macro NSIS_HOOK_PREUNINSTALL
  DetailPrint "Kova: menghapus perintah 'kova' dan 'servel' dari PATH user..."
  nsExec::ExecToLog 'powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$INSTDIR\cli\install-path.ps1" -Action remove -Dir "$INSTDIR\cli\bin"'
  Pop $0
  DetailPrint "Kova: uninstall — .wslconfig dipertahankan (user data)."
!macroend

; ---------------------------------------------------------------------------
; Helper: tulis template lengkap ke $PROFILE\.wslconfig (mode write/truncate).
; Asumsi: $1 dan $2 boleh dipakai bebas (sudah di-push oleh caller).
; ---------------------------------------------------------------------------
Function ServelWriteWslConfigTemplate
  ClearErrors
  FileOpen $1 "$PROFILE\.wslconfig" w
  ${If} ${Errors}
    DetailPrint "Kova: gagal buat .wslconfig (permission?)."
    Return
  ${EndIf}
  Call ServelWriteWslConfigBody
  FileClose $1
FunctionEnd

; Helper: tulis isi body memakai handle di $1 (mode write atau append).
Function ServelWriteWslConfigBody
  FileWrite $1 "[wsl2]$\r$\n"
  FileWrite $1 "memory=4GB$\r$\n"
  FileWrite $1 "processors=8$\r$\n"
  FileWrite $1 "swap=1GB$\r$\n"
  FileWrite $1 "guiApplications=false$\r$\n"
  FileWrite $1 "nestedVirtualization=false$\r$\n"
  FileWrite $1 "$\r$\n"
  FileWrite $1 "[experimental]$\r$\n"
  FileWrite $1 "sparseVhd=true$\r$\n"
  FileWrite $1 "autoMemoryReclaim=gradual$\r$\n"
FunctionEnd
