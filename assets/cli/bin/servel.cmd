@echo off
rem servel: alias lama untuk kova (selama v3.x). Skrip aslinya: ..\servel.ps1
setlocal
set KOVA_CLI_ALIAS=servel
where pwsh >nul 2>nul
if %errorlevel%==0 (
  pwsh -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0..\servel.ps1" %*
) else (
  powershell -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0..\servel.ps1" %*
)
exit /b %errorlevel%
