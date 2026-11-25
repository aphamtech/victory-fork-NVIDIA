@echo off
rem ---------------------------------------------------------------------------
rem Shared settings for the launcher scripts in this folder.
rem Called by the others; safe to run on its own.
rem ---------------------------------------------------------------------------

rem Repository root: the parent of this script's folder.
for %%I in ("%~dp0..") do set "VICTORY_REPO=%%~fI"

rem Your own dump of the title. Must stay OUTSIDE the repository.
if not defined VICTORY_GAME_DIR set "VICTORY_GAME_DIR=K:\GAME OFFLINE\Rockstar\GTA VI\GTA6.Xbox.v1.0.2.1-CYBERLEEK"

rem The title executable inside that dump.
if not defined VICTORY_TITLE_EXE set "VICTORY_TITLE_EXE=%VICTORY_GAME_DIR%\game_scarlett_master.exe"

rem Configuration file. VICTORY_CONFIG wins over .\victory.toml, so keep it
rem pointing at the file you actually edited.
if not defined VICTORY_CONFIG set "VICTORY_CONFIG=%VICTORY_REPO%\victory.toml"

set "VICTORY_RELEASE=%VICTORY_REPO%\target\release\victory.exe"
set "VICTORY_DIAG=%VICTORY_REPO%\target\diag\victory.exe"

rem victory.toml is looked up in the current directory by default.
cd /d "%VICTORY_REPO%" || exit /b 1

exit /b 0
