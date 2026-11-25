@echo off
rem ---------------------------------------------------------------------------
rem Show the resolved paths, check them, and report what is missing.
rem Nothing is launched. Extra arguments go to `victory`:
rem
rem   scripts\check-setup.bat
rem   scripts\check-setup.bat census --missing
rem ---------------------------------------------------------------------------
setlocal EnableExtensions
call "%~dp0config.bat" || goto :fail

call :ensure_release || goto :fail

if "%~1"=="" (
    "%VICTORY_RELEASE%" config
) else (
    "%VICTORY_RELEASE%" %*
)
set "RC=%ERRORLEVEL%"

echo.
if exist "%VICTORY_TITLE_EXE%" (
    echo title : %VICTORY_TITLE_EXE%
) else (
    echo title : MISSING - %VICTORY_TITLE_EXE%
)

if not defined VICTORY_NO_PAUSE pause
exit /b %RC%

:ensure_release
if defined VICTORY_REBUILD goto :build_release
if exist "%VICTORY_RELEASE%" exit /b 0
:build_release
echo Building victory ^(release^), this takes a few minutes...
cargo build --release -p victory || exit /b 1
exit /b 0

:fail
echo.
pause
exit /b 1
