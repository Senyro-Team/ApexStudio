#ifndef AppVersion
  #define AppVersion "0.1.1"
#endif
#ifndef StageDir
  #define StageDir "..\\..\\..\\..\\target\\release\\inno-stage"
#endif

#define AppName "Apex Studio"
#define AppId "{8B92B6E5-4187-4E52-A9A9-1DD22973A354}"
#define AppPublisher "Apex Designs"
#define AppExeName "apex-desktop.exe"
#define WebView2ClientId "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"

[Setup]
AppId={{#AppId}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppPublisher}
DefaultDirName={localappdata}\Apex Studio
DefaultGroupName=Apex Studio
DisableProgramGroupPage=yes
UsePreviousAppDir=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
WizardStyle=modern dark includetitlebar hidebevels
DisableWelcomePage=no
SetupIconFile=..\icons\icon.ico
WizardImageFile=..\icons\icon.png
WizardSmallImageFile=..\icons\icon.png
UninstallDisplayIcon={app}\{#AppExeName}
OutputDir=..\..\..\..\target\release\bundle\inno
OutputBaseFilename=Apex Studio_{#AppVersion}_x64-setup
Compression=lzma2/ultra64
SolidCompression=yes
SetupLogging=yes
CloseApplications=yes
RestartApplications=no
Uninstallable=yes
AllowNoIcons=yes
VersionInfoCompany={#AppPublisher}
VersionInfoDescription=Apex Studio Installer
VersionInfoProductName={#AppName}
VersionInfoProductVersion={#AppVersion}
VersionInfoVersion={#AppVersion}.0

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "german"; MessagesFile: "compiler:Languages\German.isl"

[Messages]
english.WelcomeLabel1=Welcome to Apex Studio
german.WelcomeLabel1=Willkommen bei Apex Studio
english.WelcomeLabel2=Projects, assets, Roblox uploads and ERLCX publishing in one workspace.
german.WelcomeLabel2=Projekte, Assets, Roblox-Uploads und ERLCX-Veröffentlichungen an einem Ort.
english.SelectComponentsDesc=Choose features for first-run setup.
german.SelectComponentsDesc=Wähle Funktionen für die erste Einrichtung.

[CustomMessages]
english.FullInstall=Full installation
german.FullInstall=Vollständige Installation
english.CustomInstall=Custom installation
german.CustomInstall=Benutzerdefiniert
english.RobloxComponent=Roblox uploads and Group Revenue
german.RobloxComponent=Roblox-Uploads und Group Revenue
english.ErlcxComponent=ERLCX uploads with the Chrome extension
german.ErlcxComponent=ERLCX-Uploads mit der Chrome-Erweiterung
english.LaunchApex=Launch Apex Studio
german.LaunchApex=Apex Studio starten
english.WebView2Title=Microsoft WebView2 Runtime
german.WebView2Title=Microsoft WebView2 Runtime
english.PrepareWebView2=The runtime is required to launch Apex Studio. Downloading it now...
german.PrepareWebView2=Die Runtime wird zum Starten von Apex Studio benötigt und jetzt heruntergeladen...
english.WebView2DownloadFailed=The Microsoft WebView2 Runtime could not be downloaded. Check your internet connection and run the installer again.
german.WebView2DownloadFailed=Die Microsoft WebView2 Runtime konnte nicht heruntergeladen werden. Prüfe deine Internetverbindung und starte das Setup erneut.
english.WebView2InstallFailed=The Microsoft WebView2 Runtime could not be installed. Install it from Microsoft and run the Apex Studio installer again.
german.WebView2InstallFailed=Die Microsoft WebView2 Runtime konnte nicht installiert werden. Installiere sie von Microsoft und starte das Apex-Studio-Setup erneut.

[Types]
Name: "full"; Description: "{cm:FullInstall}"
Name: "custom"; Description: "{cm:CustomInstall}"; Flags: iscustom

[Components]
Name: "roblox"; Description: "{cm:RobloxComponent}"; Types: full custom
Name: "erlcx"; Description: "{cm:ErlcxComponent}"; Types: full custom

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#StageDir}\{#AppExeName}"; DestDir: "{app}"; Flags: ignoreversion restartreplace
Source: "{#StageDir}\apex-chrome-bridge.exe"; DestDir: "{app}"; Flags: ignoreversion restartreplace
Source: "{#StageDir}\apex-erlcx-extension.zip"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Apex Studio\Apex Studio"; Filename: "{app}\{#AppExeName}"; WorkingDir: "{app}"
Name: "{autodesktop}\Apex Studio"; Filename: "{app}\{#AppExeName}"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExeName}"; Description: "{cm:LaunchApex}"; Flags: nowait postinstall skipifsilent

[Code]
const
  WebView2RuntimeUrl = 'https://go.microsoft.com/fwlink/p/?LinkId=2124703';
  SetupRegistryKey = 'Software\ApexStudio\Setup';
  NsIsUninstallKey = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\Apex Studio';
  InnoUninstallKey = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\{#AppId}_is1';

var
  FreshInstall: Boolean;
  PreviousInstallDir: String;
  WebView2DownloadPage: TDownloadWizardPage;

function HasWebView2At(const RootKey: Integer; const SubKeyName: String): Boolean;
var
  RuntimeVersion: String;
begin
  Result := RegQueryStringValue(RootKey, SubKeyName, 'pv', RuntimeVersion) and
    (RuntimeVersion <> '') and (RuntimeVersion <> '0.0.0.0');
end;

function IsWebView2Installed(): Boolean;
var
  ClientKey: String;
begin
  ClientKey := 'SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{#WebView2ClientId}';
  Result := HasWebView2At(HKLM, ClientKey) or
    HasWebView2At(HKCU, 'Software\Microsoft\EdgeUpdate\Clients\{#WebView2ClientId}');
end;

function InitializeSetup(): Boolean;
begin
  FreshInstall := not RegKeyExists(HKCU, NsIsUninstallKey) and
    not RegKeyExists(HKCU, InnoUninstallKey);
  PreviousInstallDir := '';
  if RegQueryStringValue(HKCU, NsIsUninstallKey, 'InstallLocation', PreviousInstallDir) then
  begin
    PreviousInstallDir := Trim(PreviousInstallDir);
    if (Length(PreviousInstallDir) > 1) and
      (PreviousInstallDir[1] = '"') and
      (PreviousInstallDir[Length(PreviousInstallDir)] = '"') then
      PreviousInstallDir := Copy(PreviousInstallDir, 2, Length(PreviousInstallDir) - 2);
  end;
  Result := True;
end;

procedure InitializeWizard();
begin
  if (PreviousInstallDir <> '') and DirExists(PreviousInstallDir) then
    WizardForm.DirEdit.Text := PreviousInstallDir;
  WebView2DownloadPage := CreateDownloadPage(
    CustomMessage('WebView2Title'), CustomMessage('PrepareWebView2'), nil);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Bootstrapper: String;
  ErrorCode: Integer;
begin
  Result := '';
  if IsWebView2Installed() then
    Exit;

  try
    WebView2DownloadPage.Clear();
    WebView2DownloadPage.Add(WebView2RuntimeUrl, 'MicrosoftEdgeWebview2Setup.exe', '');
    WebView2DownloadPage.Show();
    try
      WebView2DownloadPage.Download();
    finally
      WebView2DownloadPage.Hide();
    end;
  except
    Result := CustomMessage('WebView2DownloadFailed');
    Exit;
  end;
  Bootstrapper := ExpandConstant('{tmp}\MicrosoftEdgeWebview2Setup.exe');

  if not Exec(Bootstrapper, '/silent /install', '', SW_HIDE, ewWaitUntilTerminated, ErrorCode) then
    Result := CustomMessage('WebView2InstallFailed')
  else if ErrorCode = 3010 then
    NeedsRestart := True
  else if ErrorCode <> 0 then
    Result := CustomMessage('WebView2InstallFailed');
end;

function SelectedSetupFeatures(): String;
begin
  Result := '';
  if WizardIsComponentSelected('roblox') then
    Result := 'roblox';
  if WizardIsComponentSelected('erlcx') then
  begin
    if Result <> '' then
      Result := Result + ',';
    Result := Result + 'erlcx';
  end;
  if Result = '' then
    Result := 'none';
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  InstallDir: String;
begin
  if CurStep = ssPostInstall then
  begin
    InstallDir := ExpandConstant('{app}');
    if FreshInstall then
      RegWriteStringValue(HKCU, SetupRegistryKey, 'InitialSetupFeatures', SelectedSetupFeatures());

    if (PreviousInstallDir <> '') and (CompareText(PreviousInstallDir, InstallDir) = 0) then
    begin
      RegDeleteKeyIncludingSubkeys(HKCU, NsIsUninstallKey);
      RegDeleteKeyIncludingSubkeys(HKCU, 'Software\ApexStudio\Apex Studio');
    end;
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
  begin
    RegDeleteValue(HKCU, SetupRegistryKey, 'InitialSetupFeatures');
    RegDeleteKeyIfEmpty(HKCU, SetupRegistryKey);
  end;
end;
