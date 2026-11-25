@echo off
rem ---------------------------------------------------------------------------
rem Launch the title with the release build: the normal way to play.
rem Shaders are precompiled first, so the loading screen does not wait.
rem
rem   scripts\launch-game.bat             launch
rem   scripts\launch-game.bat -foo bar    launch, passing -foo bar to the title
rem
rem   set VICTORY_REBUILD=1
rem   scripts\launch-game.bat             rebuild first
rem ---------------------------------------------------------------------------
setlocal EnableExtensions
call "%~dp0config.bat" || goto :fail

if not exist "%VICTORY_TITLE_EXE%" (
    echo error: title executable not found:
    echo        %VICTORY_TITLE_EXE%
    echo Set VICTORY_GAME_DIR or edit scripts\config.bat.
    goto :fail
)

call :ensure_release || goto :fail

echo Launching %VICTORY_TITLE_EXE%
if "%~1"=="" (
    "%VICTORY_RELEASE%" run "%VICTORY_TITLE_EXE%" --execute --precompile
) else (
    "%VICTORY_RELEASE%" run "%VICTORY_TITLE_EXE%" --execute --precompile -- %*
)
set "RC=%ERRORLEVEL%"

if not "%RC%"=="0" (
    echo.
    echo victory exited with status %RC%.
    echo See crashes.log in the data folder ^(victory.exe config shows where^).
    goto :fail
)
exit /b 0

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
