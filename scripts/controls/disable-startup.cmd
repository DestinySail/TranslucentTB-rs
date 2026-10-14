@echo off
"%~dp0clearbar.exe" --disable-startup
if errorlevel 1 exit /b 1
"%~dp0clearbar.exe" --stop
