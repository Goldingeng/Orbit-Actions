#define AppName "Orbit Actions"
#ifndef AppVersion
  #define AppVersion "0.1.6"
#endif
#ifndef Target
  #define Target "..\target"
#endif

[Setup]
AppId={{BFC41669-E4D9-4F11-ACB8-FE1300DCDD10}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher=Goldingeng
AppPublisherURL=https://github.com/Goldingeng/Orbit-Actions
AppSupportURL=https://github.com/Goldingeng/Orbit-Actions/issues
AppUpdatesURL=https://github.com/Goldingeng/Orbit-Actions/releases
DefaultDirName={localappdata}\{#AppName}
DisableDirPage=yes
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x86compatible
ArchitecturesInstallIn64BitMode=x64os arm64
OutputDir={#Target}\installer
OutputBaseFilename=Orbit-Actions-{#AppVersion}-setup
SetupIconFile=..\assets\brand\app.ico
UninstallDisplayIcon={app}\orbit-actions.exe
UninstallDisplayName={#AppName}
LicenseFile=..\LICENSE
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
CloseApplications=force
RestartApplications=no

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "ru"; MessagesFile: "compiler:Languages\Russian.isl"

[CustomMessages]
en.Autostart=Start with Windows
ru.Autostart=Запускать вместе с Windows
en.Desktop=Settings shortcut on the desktop
ru.Desktop=Ярлык настроек на рабочем столе
en.Launch=Start Orbit Actions
ru.Launch=Запустить Orbit Actions
en.Settings=Orbit Actions settings
ru.Settings=Настройки Orbit Actions

[Tasks]
Name: "autostart"; Description: "{cm:Autostart}"
Name: "desktop"; Description: "{cm:Desktop}"; Flags: unchecked

[Files]
Source: "{#Target}\aarch64-pc-windows-msvc\release\orbit-actions.exe"; DestDir: "{app}"; Check: IsArm64; Flags: ignoreversion
Source: "{#Target}\aarch64-pc-windows-msvc\release\orbit-actions-settings.exe"; DestDir: "{app}"; Check: IsArm64; Flags: ignoreversion
Source: "{#Target}\release\orbit-actions.exe"; DestDir: "{app}"; Check: IsX64OS; Flags: ignoreversion
Source: "{#Target}\release\orbit-actions-settings.exe"; DestDir: "{app}"; Check: IsX64OS; Flags: ignoreversion
Source: "{#Target}\i686-pc-windows-msvc\release\orbit-actions.exe"; DestDir: "{app}"; Check: IsPlainX86; Flags: ignoreversion
Source: "{#Target}\i686-pc-windows-msvc\release\orbit-actions-settings.exe"; DestDir: "{app}"; Check: IsPlainX86; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\THIRD_PARTY.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\orbit-actions-settings.exe"; Comment: "{cm:Settings}"
Name: "{autodesktop}\{cm:Settings}"; Filename: "{app}\orbit-actions-settings.exe"; Tasks: desktop
Name: "{userstartup}\{#AppName}"; Filename: "{app}\orbit-actions.exe"; WorkingDir: "{app}"; Tasks: autostart

[Registry]
Root: HKCU; Subkey: "Software\Classes\orbit-actions"; Flags: uninsdeletekey dontcreatekey

[Run]
Filename: "{app}\orbit-actions.exe"; WorkingDir: "{app}"; Description: "{cm:Launch}"; Flags: nowait postinstall

[UninstallRun]
Filename: "{sys}\taskkill.exe"; Parameters: "/F /IM orbit-actions.exe /IM orbit-actions-settings.exe"; Flags: runhidden; RunOnceId: "StopOrbit"

[Code]
function IsPlainX86: Boolean;
begin
  Result := not IsArm64 and not IsX64OS;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Code: Integer;
begin
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM orbit-actions.exe /IM orbit-actions-settings.exe', '', SW_HIDE, ewWaitUntilTerminated, Code);
  Result := '';
end;
