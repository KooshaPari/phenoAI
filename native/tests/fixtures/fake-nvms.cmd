@echo off
REM fake-nvms.cmd — Windows equivalent of fake-nvms.sh.
if defined FAKE_MARKER (
  echo %FAKE_MARKER%
) else (
  echo FAKE-NVMS-ALIVE rev-1
)
if "%HERMETIC_QUIET%"=="1" exit /b 0
set SLEEP_SECS=%HERMETIC_SLEEP_SECS%
if "%SLEEP_SECS%"=="" set SLEEP_SECS=30
ping -n %SLEEP_SECS% 127.0.0.1 >nul
echo FAKE-NVMS-DONE
exit /b 0
