#ifndef AppVersion
  #error AppVersion is required
#endif
#ifndef SourceDir
  #error SourceDir is required
#endif
#ifndef OutputDir
  #error OutputDir is required
#endif
[Setup]
AppId={{D4A0B389-EC2C-43C4-9716-2C3461A976C5}
AppName=Echo
AppVersion={#AppVersion}
AppPublisher=Echo
DefaultDirName={localappdata}\Programs\Echo
DefaultGroupName=Echo
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.17763
OutputDir={#OutputDir}
OutputBaseFilename=Echo-{#AppVersion}-windows-x64-setup
SetupIconFile=..\Echo.ico
UninstallDisplayIcon={app}\Echo.exe
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
CloseApplicationsFilter=Echo.exe
RestartApplications=no

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Files]
Source: "{#SourceDir}\Echo.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\README.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion

Source: "{#SourceDir}\licenses\*"; DestDir: "{app}\licenses"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "{#SourceDir}\THIRD_PARTY.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Echo"; Filename: "{app}\Echo.exe"
Name: "{autodesktop}\Echo"; Filename: "{app}\Echo.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\Echo.exe"; Description: "Open Echo"; Flags: nowait postinstall skipifsilent

; The uninstaller deliberately never deletes {localappdata}\Echo (user notes).
