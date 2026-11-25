@echo off
rem ---------------------------------------------------------------------------
rem Launch with the faster switch set measured in the README (run 253):
rem 8 to 10 fps instead of the several seconds a frame of the plain defaults.
rem Every switch is off by default upstream, so this script turns them on.
rem ---------------------------------------------------------------------------
setlocal EnableExtensions

set "VICTORY_LEVELS_ON_DEMAND=1"
set "VICTORY_ASYNC_SUBMIT=1"
set "VICTORY_QUEUE2=1"
set "VICTORY_INDIRECT_WINDOW=1"
set "VICTORY_TEXTURE_NO_WAIT=1"
set "VICTORY_MERGE_GAP=64"
set "VICTORY_MERGE_RUNS=4096"
set "VICTORY_STAGING_POOL=0"
set "VICTORY_COPY_PLAIN=1"
set "VICTORY_JOIN_PARTS=1"
set "VICTORY_BACK_OWN_VIEW=1"
set "VICTORY_INDIRECT_GATHER=1"
set "VICTORY_CARD_REHASH_RUNS=1"

call "%~dp0launch-game.bat" %*
exit /b %ERRORLEVEL%
