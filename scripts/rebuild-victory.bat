@echo off
rem ---------------------------------------------------------------------------
rem Rebuild victory.exe from source.
rem
rem   scripts\rebuild-victory.bat          release build   -> target\release
rem   scripts\rebuild-victory.bat diag     diagnostics     -> target\diag
rem
rem The diagnostics build is release code generation plus the `diagnostics`
rem feature (frame log, card meter, profiler, capture, logging).
rem ---------------------------------------------------------------------------
setlocal EnableExtensions

rem Keep the window open when double-clicked, but not when run from a terminal.
set "PAUSE_AT_END="
echo %cmdcmdline% | find /i "%~nx0" >nul && set "PAUSE_AT_END=1"
if defined VICTORY_NO_PAUSE set "PAUSE_AT_END="

where cargo >nul 2>&1
if errorlevel 1 (
    echo error: cargo was not found on PATH.
    echo Install Rust from https://rustup.rs and open a new terminal.
    goto :fail
)

call "%~dp0config.bat" || goto :fail

if /i "%~1"=="diag" goto :diag

set "LABEL=release"
set "TARGET=%VICTORY_RELEASE%"
set "CARGO_ARGS=--release -p victory"
set "LAUNCH=launch-game.bat"
goto :build

:diag
set "LABEL=diag"
set "TARGET=%VICTORY_DIAG%"
set "CARGO_ARGS=--profile diag -p victory --features diagnostics"
set "LAUNCH=launch-game-diagnostics.bat"
goto :build

:build
echo Building victory ^(%LABEL%^) in %VICTORY_REPO%
echo.
cargo build %CARGO_ARGS%
if errorlevel 1 goto :fail

if not exist "%TARGET%" (
    echo error: cargo reported success but %TARGET% is missing.
    goto :fail
)

echo.
for %%F in ("%TARGET%") do echo Built %%~fF
for %%F in ("%TARGET%") do echo   modified %%~tF, %%~zF bytes
"%TARGET%" --version || goto :fail

echo.
echo Ready. Launch it with scripts\%LAUNCH%
if defined PAUSE_AT_END pause
exit /b 0

:fail
echo.
echo Build failed.
if defined PAUSE_AT_END pause
exit /b 1
