@echo off
rem servel CLI shim (cmd / PowerShell). Skrip aslinya: ..\servel.ps1
setlocal
where pwsh >nul 2>nul
if %errorlevel%==0 (
  pwsh -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0..\servel.ps1" %*
) else (
  powershell -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0..\servel.ps1" %*
)
exit /b %errorlevel%
