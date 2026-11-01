#define AppName "Minty Launcher"
#ifndef AppVersion
  #define AppVersion "0.5.0"
#endif

[Setup]
AppId={{C7AE79C6-8156-4EE7-9F74-7F7B78E9DA31}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=Minty Project
DefaultDirName={localappdata}\Programs\Minty Launcher
DefaultGroupName=Minty Launcher
UninstallDisplayName=Minty Launcher
UninstallDisplayIcon={app}\minty-launcher.exe
OutputDir=..\..\dist
OutputBaseFilename=MintyLauncher-Setup
SetupIconFile=Minty.ico
LicenseFile=..\..\LICENSE.txt
WizardStyle=modern
WizardResizable=yes
Compression=lzma2
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=lowest
DisableProgramGroupPage=yes
LanguageDetectionMethod=uilanguage
ShowLanguageDialog=yes
UsePreviousAppDir=yes
Uninstallable=yes
MinVersion=10.0
SetupLogging=yes

[Languages]
Name: "ru"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "en"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
ru.DesktopShortcut=Создать ярлык на Рабочем столе
ru.StartMenuShortcut=Добавить ярлык в меню «Пуск»
ru.CloseApp=Пожалуйста, закрой Minty Launcher перед установкой или обновлением.
ru.DiskSpace=Недостаточно свободного места на диске. Нужно не менее 150 МБ.
ru.Launch=Запустить Minty Launcher сейчас
ru.WelcomeTitle=Добро пожаловать в Minty Launcher!
ru.WelcomeText=Мастер установит лаунчер Minecraft Java Edition на этот компьютер.
ru.FinishText=Установка завершена. Нажми «Готово», чтобы закрыть мастер.
en.DesktopShortcut=Create a desktop shortcut
en.StartMenuShortcut=Add a shortcut to the Start menu
en.CloseApp=Please close Minty Launcher before installing or updating.
en.DiskSpace=Not enough free disk space. At least 150 MB is required.
en.Launch=Launch Minty Launcher now
en.WelcomeTitle=Welcome to Minty Launcher!
en.WelcomeText=This wizard will install the Minecraft Java Edition launcher.
en.FinishText=Installation is complete. Click Finish to close the wizard.

[Tasks]
Name: "desktopicon"; Description: "{cm:DesktopShortcut}"; GroupDescription: "Shortcuts:"
Name: "startmenuicon"; Description: "{cm:StartMenuShortcut}"; GroupDescription: "Shortcuts:"; Flags: checkedonce

[Files]
Source: "..\..\dist\minty-launcher.exe"; DestDir: "{app}"; DestName: "minty-launcher.exe"; Flags: ignoreversion

[Icons]
Name: "{userdesktop}\Minty Launcher"; Filename: "{app}\minty-launcher.exe"; Tasks: desktopicon
Name: "{group}\Minty Launcher"; Filename: "{app}\minty-launcher.exe"; Tasks: startmenuicon

[Run]
Filename: "{app}\minty-launcher.exe"; Description: "{cm:Launch}"; Flags: postinstall nowait skipifsilent

[Code]
procedure InitializeWizard();
begin
  WizardForm.WelcomeLabel1.Caption := ExpandConstant('{cm:WelcomeTitle}');
  WizardForm.WelcomeLabel2.Caption := ExpandConstant('{cm:WelcomeText}');
  WizardForm.FinishedLabel.Caption := ExpandConstant('{cm:FinishText}');
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  FreeBytes, TotalBytes: Int64;
  InstallDrive: String;
begin
  Result := '';
  if CheckForMutexes('Local\MintyLauncherAppMutex') then
  begin
    Result := ExpandConstant('{cm:CloseApp}');
    Exit;
  end;
  InstallDrive := ExtractFileDrive(WizardDirValue);
  if (InstallDrive = '') or
     (not GetSpaceOnDisk64(InstallDrive, FreeBytes, TotalBytes)) or
     (FreeBytes < 150 * 1024 * 1024) then
  begin
    Result := ExpandConstant('{cm:DiskSpace}');
    Exit;
  end;
  if not IsWin64 then
  begin
    Result := 'Minty Launcher requires 64-bit Windows.';
    Exit;
  end;
end;
