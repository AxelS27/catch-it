; Per-user Windows installer. Setup only chooses a writable app folder.
#define AppVersion "0.1.0"
#define Root ".."
#ifndef BinaryPath
  #define BinaryPath "..\\target\\release\\catch-it.exe"
#endif

[Setup]
AppId={{63A637BC-4A49-445A-AADF-457225F73DAA}
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

[Icons]
Name: "{group}\Catch It"; Filename: "{app}\catch-it.exe"; WorkingDir: "{app}"; IconFilename: "{app}\assets\catch-it.ico"

[Run]
Filename: "{app}\catch-it.exe"; Description: "Launch Catch It"; Flags: nowait postinstall skipifsilent

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
