; Loka — Windows installer
;
; Builds loka-setup-x64.exe with Inno Setup 6. Install the database alone,
; or the database plus one inference model chosen from installer/models.toml.
; The model choices are generated: run installer/gen_models.py first (it
; writes models.components.iss and models.code.iss next to this file).
;
; Build locally:
;     python installer\gen_models.py
;     ISCC.exe installer\loka.iss /DLokaVersion=0.4.0 ^
;         /DLokaBinary=target\release\loka.exe
;
; CI builds it from .github/workflows/release.yml.

#ifndef LokaVersion
  #define LokaVersion "0.0.0-dev"
#endif

#ifndef LokaBinary
  #define LokaBinary "target\release\loka.exe"
#endif

#ifndef LokaStudio
  #define LokaStudio "loka-studio\electron\dist\Loka Studio.exe"
#endif

#ifndef SourceRoot
  #define SourceRoot ".."
#endif

[Setup]
AppId={{821B6C49-1BBF-4E3F-AE7C-6DDFBC3931DC}
AppName=Loka
AppVersion={#LokaVersion}
AppPublisher=Emma Leonhart
AppPublisherURL=https://loka.emmaleonhart.com
AppSupportURL=https://github.com/EmmaLeonhart/Loka/issues
AppUpdatesURL=https://github.com/EmmaLeonhart/Loka/releases
DefaultDirName={autopf}\Loka
DefaultGroupName=Loka
DisableProgramGroupPage=yes
SourceDir={#SourceRoot}
LicenseFile=LICENSE
OutputDir=dist\installer
OutputBaseFilename=loka-setup-x64
Compression=lzma2/ultra
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=admin
UninstallDisplayIcon={app}\loka.exe
UninstallDisplayName=Loka {#LokaVersion}

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

; [Types] and [Components]: one exclusive component per model in models.toml.
#include "models.components.iss"

[Tasks]
Name: "addtopath"; Description: "Add Loka to the system PATH"; GroupDescription: "Shell integration:"
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked

[Files]
Source: "{#LokaBinary}";        DestDir: "{app}"; DestName: "loka.exe"; Flags: ignoreversion; Components: engine
Source: "{#LokaStudio}";        DestDir: "{app}"; DestName: "Loka Studio.exe"; Flags: ignoreversion; Components: engine
Source: "installer\models.toml"; DestDir: "{app}"; Flags: ignoreversion; Components: engine
Source: "LICENSE";              DestDir: "{app}"; Flags: ignoreversion; Components: engine
Source: "README.md";            DestDir: "{app}"; Flags: ignoreversion; Components: engine

[Icons]
Name: "{group}\Loka Studio"; Filename: "{app}\Loka Studio.exe"; WorkingDir: "{app}"
Name: "{group}\Loka shell";  Filename: "cmd.exe"; Parameters: "/k ""{app}\loka.exe"" --help"; WorkingDir: "{app}"
Name: "{group}\Loka website"; Filename: "https://loka.emmaleonhart.com"
Name: "{group}\Uninstall Loka"; Filename: "{uninstallexe}"
Name: "{autodesktop}\Loka Studio"; Filename: "{app}\Loka Studio.exe"; WorkingDir: "{app}"; Tasks: desktopicon
Name: "{autodesktop}\Loka shell"; Filename: "cmd.exe"; Parameters: "/k ""{app}\loka.exe"" --help"; WorkingDir: "{app}"; Tasks: desktopicon

[Registry]
; PATH entry written via [Code] so we can clean it up on uninstall —
; the built-in {olddata} pattern is fragile across user/system PATH.

[Code]
const
  EnvKey = 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment';

procedure AddToSystemPath(const Dir: string);
var
  CurrentPath: string;
begin
  if not RegQueryStringValue(HKEY_LOCAL_MACHINE, EnvKey, 'Path', CurrentPath) then
    CurrentPath := '';
  if Pos(';' + LowerCase(Dir) + ';', ';' + LowerCase(CurrentPath) + ';') = 0 then
  begin
    if (Length(CurrentPath) > 0) and (CurrentPath[Length(CurrentPath)] <> ';') then
      CurrentPath := CurrentPath + ';';
    CurrentPath := CurrentPath + Dir;
    RegWriteExpandStringValue(HKEY_LOCAL_MACHINE, EnvKey, 'Path', CurrentPath);
  end;
end;

procedure RemoveFromSystemPath(const Dir: string);
var
  CurrentPath: string;
  P: Integer;
  Needle: string;
begin
  if not RegQueryStringValue(HKEY_LOCAL_MACHINE, EnvKey, 'Path', CurrentPath) then
    Exit;
  Needle := ';' + LowerCase(Dir);
  P := Pos(Needle + ';', ';' + LowerCase(CurrentPath) + ';');
  if P = 0 then
    P := Pos(Needle, ';' + LowerCase(CurrentPath));
  if P > 0 then
  begin
    StringChangeEx(CurrentPath, Dir + ';', '', True);
    StringChangeEx(CurrentPath, ';' + Dir, '', True);
    StringChangeEx(CurrentPath, Dir, '', True);
    RegWriteExpandStringValue(HKEY_LOCAL_MACHINE, EnvKey, 'Path', CurrentPath);
  end;
end;

#include "models.code.iss"

procedure WriteInstallManifest(const Dir: string);
var
  Lines: TArrayOfString;
  WantModel: Boolean;
  ModelId, ModelRepo: String;
begin
  WantModel := SelectedModel(ModelId, ModelRepo);
  SetArrayLength(Lines, 6);
  Lines[0] := '# Written by the Loka installer. Read by loka.exe on first launch';
  Lines[1] := '# to decide whether to pull an inference model from Hugging Face.';
  Lines[2] := '';
  if WantModel then
  begin
    Lines[3] := 'install_model = true';
    Lines[4] := 'model_id      = "' + ModelId + '"';
    Lines[5] := 'model_repo    = "' + ModelRepo + '"';
  end
  else
  begin
    Lines[3] := 'install_model = false';
    Lines[4] := 'model_id      = ""';
    Lines[5] := 'model_repo    = ""';
  end;
  SaveStringsToFile(Dir + '\install-selection.toml', Lines, False);
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
  begin
    WriteInstallManifest(ExpandConstant('{app}'));
    if WizardIsTaskSelected('addtopath') then
      AddToSystemPath(ExpandConstant('{app}'));
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
    RemoveFromSystemPath(ExpandConstant('{app}'));
end;
