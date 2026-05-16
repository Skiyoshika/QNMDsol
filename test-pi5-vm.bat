@echo off
REM Pi 5 VM smoke-test one-click runner. Routes to the existing PowerShell
REM scripts under Neurostick-Pi-5\scripts. Double-click to run; pass arguments
REM to override mode.
REM
REM Modes:
REM   (no arg)   sim-only Windows host smoke (no OpenBCI dongle required)
REM   hw <COMx>  Windows host smoke including OpenBCI capture
REM   docker     buildx -> arm64 image -> Docker container sim smoke
REM   preflight  WSL/Docker availability report

setlocal
pushd "%~dp0"

set "MODE=%~1"
if "%MODE%"=="" set "MODE=sim"

if /I "%MODE%"=="preflight" (
    powershell -ExecutionPolicy Bypass -File ".\Neurostick-Pi-5\scripts\windows-docker-preflight.ps1"
    set "RC=%ERRORLEVEL%"
    goto :done
)

if /I "%MODE%"=="docker" (
    powershell -ExecutionPolicy Bypass -File ".\Neurostick-Pi-5\scripts\docker-arm64-verify.ps1"
    set "RC=%ERRORLEVEL%"
    goto :done
)

if /I "%MODE%"=="hw" (
    set "PORT=%~2"
    if "%PORT%"=="" (
        echo Usage: test-pi5-vm.bat hw COM3
        set "RC=2"
        goto :done
    )
    powershell -ExecutionPolicy Bypass -File ".\Neurostick-Pi-5\scripts\windows-smoke-test.ps1" -SerialPort "%PORT%"
    set "RC=%ERRORLEVEL%"
    goto :done
)

REM Default: sim-only smoke (skips real OpenBCI capture)
powershell -ExecutionPolicy Bypass -File ".\Neurostick-Pi-5\scripts\windows-smoke-test.ps1" -SkipHardware
set "RC=%ERRORLEVEL%"

:done
popd
endlocal & exit /b %RC%
