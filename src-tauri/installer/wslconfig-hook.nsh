; Servel NSIS installer hook
; Injects %USERPROFILE%\.wslconfig template required for Docker Desktop WSL2 backend.
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

!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Servel: configuring WSL2 (.wslconfig)..."
  Push $0   ; target path
  Push $1   ; file handle / aux
  Push $2   ; whole-file buffer / aux
  Push $R0  ; line read buffer
  Push $R1  ; found-index result

  StrCpy $0 "$PROFILE\.wslconfig"

  ${If} ${FileExists} "$0"
    MessageBox MB_YESNOCANCEL|MB_ICONQUESTION \
      "Servel ingin menulis konfigurasi WSL2 ke:$\r$\n$0$\r$\n$\r$\n\
File sudah ada.$\r$\n$\r$\n\
[Yes]    = Overwrite (file lama dibackup ke .wslconfig.bak)$\r$\n\
[No]     = Append (tambah section [wsl2]/[experimental] kalau belum ada)$\r$\n\
[Cancel] = Skip (jangan ubah)" \
      /SD IDCANCEL IDYES servel_wsl_overwrite IDNO servel_wsl_append

    DetailPrint "Servel: .wslconfig skipped by user."
    Goto servel_wsl_done

    servel_wsl_overwrite:
      DetailPrint "Servel: backing up existing .wslconfig to .wslconfig.bak"
      Delete "$0.bak"
      Rename "$0" "$0.bak"
      Call ServelWriteWslConfigTemplate
      DetailPrint "Servel: .wslconfig overwritten."
      Goto servel_wsl_done

    servel_wsl_append:
      ; Baca seluruh isi file ke $2 lalu cari "[wsl2]"
      ClearErrors
      FileOpen $1 "$0" r
      ${If} ${Errors}
        DetailPrint "Servel: gagal buka .wslconfig untuk append; skipped."
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
          DetailPrint "Servel: gagal append .wslconfig; skipped."
          Goto servel_wsl_done
        ${EndIf}
        FileSeek $1 0 END
        FileWrite $1 "$\r$\n"
        Call ServelWriteWslConfigBody
        FileClose $1
        DetailPrint "Servel: .wslconfig appended with [wsl2] section."
      ${Else}
        DetailPrint "Servel: section [wsl2] sudah ada; append dilewati."
      ${EndIf}
      Goto servel_wsl_done
  ${Else}
    DetailPrint "Servel: writing fresh .wslconfig template."
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
  DetailPrint "Servel: uninstall — .wslconfig dipertahankan (user data)."
!macroend

; ---------------------------------------------------------------------------
; Helper: tulis template lengkap ke $PROFILE\.wslconfig (mode write/truncate).
; Asumsi: $1 dan $2 boleh dipakai bebas (sudah di-push oleh caller).
; ---------------------------------------------------------------------------
Function ServelWriteWslConfigTemplate
  ClearErrors
  FileOpen $1 "$PROFILE\.wslconfig" w
  ${If} ${Errors}
    DetailPrint "Servel: gagal buat .wslconfig (permission?)."
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
