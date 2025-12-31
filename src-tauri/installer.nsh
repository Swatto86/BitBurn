!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "nsDialogs.nsh"
!include "FileFunc.nsh"

Var ContextMenuCheckbox
Var ContextMenuShouldRegister

; Hook executed before files are copied. Shows the opt-in checkbox.
!macro NSIS_HOOK_PREINSTALL
  !insertmacro MUI_HEADER_TEXT "Explorer Context Menu" "Add BitBurn -> Shred -> Choose Shred Algorithm to Explorer?"
  nsDialogs::Create 1018
  Pop $0
  ${If} $0 == "error"
    Abort
  ${EndIf}

  ${NSD_CreateCheckbox} 0u 0u 100% 12u "Add Explorer context menu entry"
  Pop $ContextMenuCheckbox
  ${NSD_Check} $ContextMenuCheckbox

  nsDialogs::Show
  ${NSD_GetState} $ContextMenuCheckbox $ContextMenuShouldRegister
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
