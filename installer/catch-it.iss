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
#ifdef UpgradeSmoke
  #ifdef InjectFileCollision
Source: "{#Root}\installer\install-layout.txt"; DestDir: "{app}"; DestName: "test-failure-hook.txt"; Flags: ignoreversion
  #endif
#endif

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
// Inno rolls back newly installed files on failure but does not restore files
// overwritten in an earlier install. Keep a complete snapshot of the shipped
// app files until ssDone, and restore it after Inno's failure cleanup.
var
  UpgradeBackup: String;
  UpgradePending, UpgradeCompleted: Boolean;

function BackupFile(const RelativeName: String): Boolean;
begin
  Result := CopyFile(AddBackslash(ExpandConstant('{app}')) + RelativeName,
    AddBackslash(UpgradeBackup) + RelativeName, True);
end;

function RestoreFile(const RelativeName: String): Boolean;
begin
  Result := CopyFile(AddBackslash(UpgradeBackup) + RelativeName,
    AddBackslash(ExpandConstant('{app}')) + RelativeName, False);
end;

function RestoreUpgrade: Boolean;
begin
  Result := RestoreFile('catch-it.exe') and
    RestoreFile('assets\catch-it-logo.png') and
    RestoreFile('assets\catch-it.ico') and
    RestoreFile('assets\google-signin-button.bmp') and
    RestoreFile('install-layout.txt');
  if Result then
    Result := DelTree(UpgradeBackup, True, True, True);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := '';
  UpgradeBackup := AddBackslash(ExpandConstant('{app}')) + '.catch-it-upgrade-backup';
  if DirExists(UpgradeBackup) then begin
    if FileExists(AddBackslash(UpgradeBackup) + 'done.txt') then begin
      if not DelTree(UpgradeBackup, True, True, True) then begin
        Result := 'Cannot remove a completed upgrade backup in ' + UpgradeBackup;
        Exit;
      end;
    end else if FileExists(AddBackslash(UpgradeBackup) + 'ready.txt') then begin
      if not RestoreUpgrade then begin
        Result := 'A previous upgrade was interrupted and could not be restored. The original files remain in ' + UpgradeBackup;
        Exit;
      end;
      Log('Recovered interrupted Catch It upgrade before installing');
    end else if FileExists(AddBackslash(UpgradeBackup) + 'in-progress.txt') then begin
      if not DelTree(UpgradeBackup, True, True, True) then begin
        Result := 'Cannot clear an unfinished upgrade backup in ' + UpgradeBackup;
        Exit;
      end;
    end else begin
      Result := 'Unexpected files in ' + UpgradeBackup + '. Move them before updating.';
      Exit;
    end;
  end;
  // First installs have nothing to roll back. Existing installs must have all
  // packaged files available for a complete, testable rollback snapshot.
  if not FileExists(AddBackslash(ExpandConstant('{app}')) + 'catch-it.exe') then Exit;
  if not ForceDirectories(AddBackslash(UpgradeBackup) + 'assets') or
     not SaveStringToFile(AddBackslash(UpgradeBackup) + 'in-progress.txt', 'backup', False) or
     not BackupFile('catch-it.exe') or
     not BackupFile('assets\catch-it-logo.png') or
     not BackupFile('assets\catch-it.ico') or
     not BackupFile('assets\google-signin-button.bmp') or
     not BackupFile('install-layout.txt') or
     not SaveStringToFile(AddBackslash(UpgradeBackup) + 'ready.txt', 'ready', False) then begin
    Result := 'Cannot safely back up the installed version. Upgrade canceled.';
    // Setup has not copied any new files yet. Remove this attempt's incomplete
    // snapshot so a later retry is not blocked by an unfinished marker.
    if DirExists(UpgradeBackup) and
       not DelTree(UpgradeBackup, True, True, True) then
      Result := Result + ' Clear the incomplete backup in ' + UpgradeBackup + ' before retrying.';
    Exit;
  end;
  UpgradePending := True;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssDone then begin
    UpgradeCompleted := True;
    if UpgradePending then begin
      // If backup deletion is interrupted, a future setup must NOT restore the
      // older executable over an installation that already succeeded.
      if not SaveStringToFile(AddBackslash(UpgradeBackup) + 'done.txt', 'done', False) then begin
        // A leftover ready marker would otherwise roll a successful upgrade
        // back to the older executable on the next launch of Setup.
        if not DeleteFile(AddBackslash(UpgradeBackup) + 'ready.txt') then
          Log('CRITICAL: Unable to mark or clear completed upgrade backup: ' + UpgradeBackup);
      end;
      if not DelTree(UpgradeBackup, True, True, True) then
        Log('Unable to remove completed upgrade backup: ' + UpgradeBackup);
    end;
  end;
end;

procedure DeinitializeSetup;
begin
  if UpgradePending and not UpgradeCompleted then begin
    if not RestoreUpgrade then
      Log('CRITICAL: Could not restore previous app files. Backup kept in ' + UpgradeBackup);
  end;
end;

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
