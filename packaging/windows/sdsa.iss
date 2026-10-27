; Inno Setup script for the Windows installer. CI passes the version:
;   iscc /DVersion=0.1.0 packaging\windows\sdsa.iss
; Paths are relative to this file.

#ifndef Version
  #define Version "0.0.0"
#endif
#define Name "San Diego Social Activities"

[Setup]
AppId={{2B9728B4-13B5-4AE7-9CAB-AD79874CDC15}
AppName={#Name}
AppVersion={#Version}
AppPublisher=Vishal Patel
AppPublisherURL=https://github.com/vishaldpatel/socialactivities
DefaultDirName={autopf}\{#Name}
DisableProgramGroupPage=yes
; Installs for the current user without asking for admin rights.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\..\dist
OutputBaseFilename=sd-social-activities-{#Version}-windows-x64-setup
SetupIconFile=..\icon.ico
UninstallDisplayIcon={app}\sdsa.exe
Compression=lzma2
SolidCompression=yes
WizardStyle=modern

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\..\target\release\sdsa.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#Name}"; Filename: "{app}\sdsa.exe"
Name: "{autodesktop}\{#Name}"; Filename: "{app}\sdsa.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\sdsa.exe"; Description: "{cm:LaunchProgram,{#Name}}"; Flags: nowait postinstall skipifsilent
