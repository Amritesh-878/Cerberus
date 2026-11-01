; Ships the console command as cerberus.com, which a shell finds before cerberus.exe, and puts the folder on the user PATH.
!macro NSIS_HOOK_POSTINSTALL
  IfFileExists "$INSTDIR\cerberus-cli.exe" 0 +3
    Delete "$INSTDIR\cerberus.com"
    Rename "$INSTDIR\cerberus-cli.exe" "$INSTDIR\cerberus.com"
  System::Call 'Kernel32::SetEnvironmentVariable(t "CERBERUS_DIR", t "$INSTDIR")'
  nsExec::ExecToLog `powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "$$d = $$env:CERBERUS_DIR.TrimEnd('\'); $$p = (Get-Item 'HKCU:\Environment').GetValue('Path', '', 'DoNotExpandEnvironmentNames'); $$keep = @($$p -split ';' | Where-Object { $$_ -and $$_.TrimEnd('\') -ne $$d }) + $$d; Set-ItemProperty 'HKCU:\Environment' -Name Path -Value ($$keep -join ';') -Type ExpandString"`
  ; SMTO_ABORTIFHUNG (0x2), since a plain broadcast waits out every hung window, up to 45 s in testing.
  System::Call 'user32::SendMessageTimeout(p 0xFFFF, i 0x1A, p 0, t "Environment", i 0x2, i 5000, *p .r0)'
!macroend

; Tauri removes the install folder before the post-uninstall hook, so the extra file goes first.
!macro NSIS_HOOK_PREUNINSTALL
  Delete "$INSTDIR\cerberus.com"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  System::Call 'Kernel32::SetEnvironmentVariable(t "CERBERUS_DIR", t "$INSTDIR")'
  nsExec::ExecToLog `powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "$$d = $$env:CERBERUS_DIR.TrimEnd('\'); $$p = (Get-Item 'HKCU:\Environment').GetValue('Path', '', 'DoNotExpandEnvironmentNames'); $$keep = @($$p -split ';' | Where-Object { $$_ -and $$_.TrimEnd('\') -ne $$d }); Set-ItemProperty 'HKCU:\Environment' -Name Path -Value ($$keep -join ';') -Type ExpandString"`
  System::Call 'user32::SendMessageTimeout(p 0xFFFF, i 0x1A, p 0, t "Environment", i 0x2, i 5000, *p .r0)'
!macroend
