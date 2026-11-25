@echo off
rem ---------------------------------------------------------------------------
rem Launch with the diagnostics build: release code generation plus the
rem `diagnostics` feature (frame log, card meter, profiler, capture, logging).
rem Built into target\diag so it does not evict the release build.
rem The first build takes a while. See README "Debugging" for the switches.
rem
rem   set VICTORY_REBUILD=1
rem   scripts\launch-game-diagnostics.bat      rebuild first
rem ---------------------------------------------------------------------------
setlocal EnableExtensions
call "%~dp0config.bat" || goto :fail

if not exist "%VICTORY_TITLE_EXE%" (
    echo error: title executable not found:
    echo        %VICTORY_TITLE_EXE%
    echo Set VICTORY_GAME_DIR or edit scripts\config.bat.
    goto :fail
)

if defined VICTORY_REBUILD goto :build
if exist "%VICTORY_DIAG%" goto :launch

:build
echo Building victory ^(diag, diagnostics^), this takes a few minutes...
cargo build --profile diag -p victory --features diagnostics || goto :fail

:launch
if not defined VICTORY_LOG set "VICTORY_LOG=victory_loader=debug,info"

echo Launching %VICTORY_TITLE_EXE% ^(diagnostics^)
if "%~1"=="" (
    "%VICTORY_DIAG%" run "%VICTORY_TITLE_EXE%" --execute --precompile
) else (
    "%VICTORY_DIAG%" run "%VICTORY_TITLE_EXE%" --execute --precompile -- %*
)
set "RC=%ERRORLEVEL%"

if not "%RC%"=="0" (
    echo.
    echo victory exited with status %RC%.
    goto :fail
)
exit /b 0

:fail
echo.
pause
exit /b 1
