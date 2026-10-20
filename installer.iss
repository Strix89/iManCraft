; Inno Setup Script per IManCraft
#define MyAppName "IManCraft"
#define MyAppVersion "0.2.0"
#define MyAppPublisher "Crafting Apps"
#define MyAppURL "https://github.com/Strix89/iManCraft"
#define MyAppExeName "imancraft.exe"

[Setup]
AppId={{E65A46BC-3435-43FE-A90C-527D7BD7573B}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
DefaultDirName={autopf}\{#MyAppName}
DisableProgramGroupPage=yes
LicenseFile=LEGGIMI.txt
OutputDir=dist
OutputBaseFilename=IManCraft-v{#MyAppVersion}-Setup
SetupIconFile=assets\imancraft.ico
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
ArchitecturesInstallIn64BitMode=x64

[Languages]
Name: "italian"; MessagesFile: "compiler:Languages\Italian.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "dist\IManCraft-v{#MyAppVersion}-win64\imancraft.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "dist\IManCraft-v{#MyAppVersion}-win64\imancraft-cli.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "dist\IManCraft-v{#MyAppVersion}-win64\sqlite3.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "dist\IManCraft-v{#MyAppVersion}-win64\LEGGIMI.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "dist\IManCraft-v{#MyAppVersion}-win64\imancraft.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "dist\IManCraft-v{#MyAppVersion}-win64\tools\*"; DestDir: "{app}\tools"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\imancraft.ico"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\imancraft.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent
