!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "FileFunc.nsh"

Var ContextMenuShouldRegister

; Hook executed before files are copied. Shows the opt-in checkbox.
!macro NSIS_HOOK_PREINSTALL
  !insertmacro MUI_HEADER_TEXT "Explorer Context Menu" "Add BitBurn -> Shred -> Choose Shred Algorithm to Explorer?"
  ; Use a simple Yes/No prompt to avoid UI hang scenarios seen with nsDialogs.
  MessageBox MB_YESNO|MB_ICONQUESTION "Add BitBurn -> Shred -> Choose Shred Algorithm to Explorer?" IDYES +2
  StrCpy $ContextMenuShouldRegister ${BST_UNCHECKED}
  Goto done_ctx_prompt
  StrCpy $ContextMenuShouldRegister ${BST_CHECKED}
done_ctx_prompt:
!macroend

; Hook executed after install finishes. Registers context menu if opted in.
!macro NSIS_HOOK_POSTINSTALL
  ${If} $ContextMenuShouldRegister == ${BST_CHECKED}
    DetailPrint "Registering Explorer context menu"
    nsExec::ExecToLog '"$INSTDIR\\BitBurn.exe" --register-context-menu'
  ${EndIf}
!macroend

; Hook executed after uninstall completes. Always remove context menu entries.
!macro NSIS_HOOK_POSTUNINSTALL
  DetailPrint "Removing Explorer context menu"
  nsExec::ExecToLog '"$INSTDIR\\BitBurn.exe" --unregister-context-menu'
!macroend
