; Per-user Windows installer. Setup only chooses a writable app folder.
#ifndef AppVersion
  #error "Build through scripts/build-installer.ps1 to set AppVersion"
#endif
#ifndef PackageAppId
  #define PackageAppId "{{63A637BC-4A49-445A-AADF-457225F73DAA}"
#endif
#define Root ".."
#ifndef BinaryPath
  #define BinaryPath "..\\target\\release\\catch-it.exe"
#endif

[Setup]
AppId={#PackageAppId}
AppName=Catch It
AppVersion={#AppVersion}
AppPublisher=Catch It
DefaultDirName={localappdata}\Programs\Catch It
DefaultGroupName=Catch It
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#Root}\dist
OutputBaseFilename=CatchIt-Setup-{#AppVersion}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
WizardImageFile={#Root}\assets\brand\wizard-side.bmp
WizardSmallImageFile={#Root}\assets\brand\wizard-small.bmp
SetupIconFile={#Root}\assets\brand\catch-it.ico
UninstallDisplayIcon={app}\assets\catch-it.ico
CloseApplications=yes
RestartApplications=no
DisableProgramGroupPage=yes
DisableReadyPage=yes

[Files]
Source: "{#BinaryPath}"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Root}\assets\brand\catch-it-logo.png"; DestDir: "{app}\assets"; Flags: ignoreversion
Source: "{#Root}\assets\brand\catch-it.ico"; DestDir: "{app}\assets"; Flags: ignoreversion
Source: "{#Root}\assets\third-party\google-signin-button.bmp"; DestDir: "{app}\assets"; Flags: ignoreversion
Source: "{#Root}\installer\install-layout.txt"; DestDir: "{app}"; Flags: ignoreversion

[Dirs]
Name: "{app}\Temp"

; Captures are explicitly temporary. Uninstall removes only the files created
; inside the app's dedicated Temp directory, not arbitrary files elsewhere.
[UninstallDelete]
Type: files; Name: "{app}\Temp\shot_*.png"
Type: files; Name: "{app}\Temp\shot_*.png.part"

; Integration-test builds use a different AppId and install directory. Do not
; launch the app or touch the user's real Startup/Start Menu shortcuts.
#ifndef UpgradeSmoke
[Tasks]
Name: "autostart"; Description: "Start Catch It when I sign in to Windows"; GroupDescription: "Startup:"

[Icons]
Name: "{group}\Catch It"; Filename: "{app}\catch-it.exe"; WorkingDir: "{app}"; IconFilename: "{app}\assets\catch-it.ico"
Name: "{userstartup}\Catch It"; Filename: "{app}\catch-it.exe"; WorkingDir: "{app}"; IconFilename: "{app}\assets\catch-it.ico"; Tasks: autostart

[Run]
Filename: "{app}\catch-it.exe"; Description: "Launch Catch It"; Flags: nowait postinstall skipifsilent
Filename: "{app}\catch-it.exe"; Flags: nowait skipifnotsilent
#endif

[Code]
// Temporary captures live under {app}\Temp, so the install directory must
// remain writable by the current user without administrator privileges.
function NextButtonClick(CurPageID: Integer): Boolean;
var
  Dir, Probe: String;
begin
  Result := True;
  if CurPageID <> wpSelectDir then Exit;
  Dir := WizardDirValue;
  if not ForceDirectories(Dir) then begin
    MsgBox('This folder cannot be created. Choose a folder in your user profile.', mbError, MB_OK);
    Result := False;
    Exit;
  end;
  Probe := AddBackslash(Dir) + '.catch-it-write-check';
  if not SaveStringToFile(Probe, 'write check', False) then begin
    MsgBox('Catch It stores temporary screenshots inside its install folder. Choose a folder you can write to without administrator access.', mbError, MB_OK);
    Result := False;
  end else
    DeleteFile(Probe);
end;
