@echo off
rem Usage: scripts\build-windows.bat [cargo args...]   (no args = cargo build --release -p selo)
rem Pick the newest MSVC toolset that has cl.exe; VS 18's default (v145) can be headers-only.
setlocal
set "CARGO_TARGET_DIR=target\windows-arm64"
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
for /f "usebackq delims=" %%i in (`"%VSWHERE%" -latest -products * -property installationPath`) do set "VSINSTALL=%%i"
if not defined VSINSTALL (
    echo Visual Studio Build Tools not found.
    exit /b 1
)
set "VCVER="
for /f "delims=" %%v in ('dir /b /ad /o-n "%VSINSTALL%\VC\Tools\MSVC" 2^>nul') do (
    if not defined VCVER if exist "%VSINSTALL%\VC\Tools\MSVC\%%v\bin\Hostarm64\arm64\cl.exe" set "VCVER=%%v"
)
if not defined VCVER (
    echo No usable MSVC toolset under "%VSINSTALL%\VC\Tools\MSVC".
    exit /b 1
)
call "%VSINSTALL%\Common7\Tools\VsDevCmd.bat" -arch=arm64 -vcvars_ver=%VCVER% -no_logo
if "%~1"=="" (
    cargo build --release -p selo
) else (
    cargo %*
)
