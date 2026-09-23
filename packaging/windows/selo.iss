#ifndef MyAppVersion
  #define MyAppVersion "0.1.0"
#endif
#ifndef MyArch
  #define MyArch "x64"
#endif
#ifndef MySource
  #define MySource "..\..\target\release\selo.exe"
#endif

[Setup]
AppName=Selo
AppVersion={#MyAppVersion}
AppPublisher=osdodo
DefaultDirName={autopf}\Selo
DefaultGroupName=Selo
UninstallDisplayIcon={app}\selo.exe
OutputDir=..\..\dist
OutputBaseFilename=Selo-{#MyAppVersion}-windows-{#MyArch}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed={#MyArch}
ArchitecturesInstallIn64BitMode={#MyArch}

[Files]
Source: "{#MySource}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Selo"; Filename: "{app}\selo.exe"
