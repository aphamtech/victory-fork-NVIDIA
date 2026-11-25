@echo off
rem ---------------------------------------------------------------------------
rem Translate every shader into the cache a run reads, so the loading screen
rem does not wait for them. Extra arguments go to `victory precompile`:
rem
rem   scripts\precompile-shaders.bat
rem   scripts\precompile-shaders.bat --jobs 12 --validate
rem
rem Run it again after a git pull that touches victory-shader.
rem ---------------------------------------------------------------------------
setlocal EnableExtensions
call "%~dp0config.bat" || goto :fail

if not exist "%VICTORY_GAME_DIR%\" (
    echo error: game folder not found: %VICTORY_GAME_DIR%
    echo Set VICTORY_GAME_DIR or edit scripts\config.bat.
    goto :fail
)

call :ensure_release || goto :fail

echo Precompiling shaders in %VICTORY_GAME_DIR%
"%VICTORY_RELEASE%" precompile "%VICTORY_GAME_DIR%" %*
if not "%ERRORLEVEL%"=="0" goto :fail

echo.
echo Done.
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
