@echo off
"%~dp0clearbar.exe" --status
type "%LOCALAPPDATA%\Clearbar\status.txt"
pause
