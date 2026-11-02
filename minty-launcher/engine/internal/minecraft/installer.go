package minecraft

import (
	"archive/zip"
	"context"
	"crypto/md5"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"runtime"
	"strings"
	"time"

	"minty-engine/internal/download"
	"minty-engine/internal/protocol"
	javart "minty-engine/internal/runtime"
	"minty-engine/internal/settings"
)

type artifact struct {
	Path string `json:"path"`
	URL  string `json:"url"`
	SHA1 string `json:"sha1"`
	Size int64  `json:"size"`
}

type libraryDownloads struct {
	Artifact    *artifact           `json:"artifact"`
	Classifiers map[string]artifact `json:"classifiers"`
}

type library struct {
	Name      string            `json:"name"`
	URL       string            `json:"url"`
	Downloads libraryDownloads  `json:"downloads"`
	Rules     []argumentRule    `json:"rules"`
	Natives   map[string]string `json:"natives"`
}

type argumentRule struct {
	Action   string            `json:"action"`
	OS       map[string]string `json:"os"`
	Features map[string]bool   `json:"features"`
}

type assetIndex struct {
	ID   string `json:"id"`
	URL  string `json:"url"`
	SHA1 string `json:"sha1"`
	Size int64  `json:"size"`
}

type versionMetadata struct {
	ID         string     `json:"id"`
	Type       string     `json:"type"`
	MainClass  string     `json:"mainClass"`
	Assets     string     `json:"assets"`
	AssetIndex assetIndex `json:"assetIndex"`
	Downloads  struct {
		Client artifact `json:"client"`
	} `json:"downloads"`
	Libraries []library `json:"libraries"`
	Arguments struct {
		JVM  []json.RawMessage `json:"jvm"`
		Game []json.RawMessage `json:"game"`
	} `json:"arguments"`
	MinecraftArguments string `json:"minecraftArguments"`
	JavaVersion        struct {
		Major int `json:"majorVersion"`
	} `json:"javaVersion"`
	Logging struct {
		Client struct {
			File struct {
				ID   string `json:"id"`
				URL  string `json:"url"`
				SHA1 string `json:"sha1"`
				Size int64  `json:"size"`
			} `json:"file"`
		} `json:"client"`
	} `json:"logging"`
}

type assetFile struct {
	Hash string `json:"hash"`
	Size int64  `json:"size"`
}

type assetManifest struct {
	Objects map[string]assetFile `json:"objects"`
}

var versionIDPattern = regexp.MustCompile(`^[A-Za-z0-9._-]+$`)

func InstallVersion(ctx context.Context, configuration settings.Settings, writer *protocol.Writer) error {
	_, _, err := prepare(ctx, configuration, writer)
	if err == nil {
		_ = writer.Send(protocol.Event{Type: "complete", Message: "Minecraft установлен."})
	}
	return err
}

func Launch(ctx context.Context, configuration settings.Settings, writer *protocol.Writer) error {
	if err := validateLaunchLoader(configuration); err != nil {
		return err
	}
	metadata, gameDir, err := prepare(ctx, configuration, writer)
	if err != nil {
		return err
	}
	major := metadata.JavaVersion.Major
	if major == 0 {
		major = requiredJava(configuration.SelectedVersion)
	}
	runtimeRoot, err := os.UserCacheDir()
	if err != nil {
		return fmt.Errorf("locate Java runtime directory: %w", err)
	}
	java, err := javart.Ensure(ctx, configuration.JavaPath, major, filepath.Join(runtimeRoot, "Minty Launcher", "runtimes"), writer)
	if err != nil {
		return err
	}

	gameVersion := metadata.ID
	assetsName := metadata.AssetIndex.ID
	if assetsName == "" {
		assetsName = metadata.Assets
	}
	versionDir := filepath.Join(configuration.GameDirectory, "versions", gameVersion)
	nativeDir := filepath.Join(configuration.GameDirectory, "natives", gameVersion)
	if err := os.MkdirAll(nativeDir, 0o755); err != nil {
		return fmt.Errorf("create native library folder: %w", err)
	}
	var classpath []string
	for _, item := range metadata.Libraries {
		if !rulesAllow(item.Rules, false) {
			continue
		}
		libraryArtifact := item.Downloads.Artifact
		if libraryArtifact == nil {
			libraryArtifact = mavenArtifact(item)
		}
		if libraryArtifact != nil && libraryArtifact.Path != "" {
			classpath = append(classpath, filepath.Join(configuration.GameDirectory, "libraries", filepath.FromSlash(libraryArtifact.Path)))
		}
		for platform, classifier := range item.Natives {
			if platform != platformName() {
				continue
			}
			classifier = strings.ReplaceAll(classifier, "${arch}", architectureBits())
			if native, ok := item.Downloads.Classifiers[classifier]; ok {
				archive := filepath.Join(configuration.GameDirectory, "libraries", filepath.FromSlash(native.Path))
				if err := extractNatives(archive, nativeDir); err != nil {
					return err
				}
			}
		}
	}
	clientJar := filepath.Join(versionDir, gameVersion+".jar")
	classpath = append(classpath, clientJar)

	instanceDir := gameDir
	if configuration.IsolatedProfiles {
		instanceDir = filepath.Join(configuration.GameDirectory, "instances", safeComponent(gameVersion))
	}
	if err := os.MkdirAll(instanceDir, 0o755); err != nil {
		return fmt.Errorf("create game instance folder: %w", err)
	}
	accountUUID := offlineUUID(configuration.Username)
	variables := map[string]string{
		"auth_player_name":    configuration.Username,
		"version_name":        gameVersion,
		"game_directory":      instanceDir,
		"assets_root":         filepath.Join(configuration.GameDirectory, "assets"),
		"assets_index_name":   assetsName,
		"auth_uuid":           accountUUID,
		"auth_access_token":   "0",
		"auth_session":        "0",
		"auth_xuid":           "",
		"clientid":            "",
		"user_type":           "legacy",
		"version_type":        metadata.Type,
		"natives_directory":   nativeDir,
		"launcher_name":       "Minty Launcher",
		"launcher_version":    "2.0.0-global",
		"classpath":           strings.Join(classpath, string(os.PathListSeparator)),
		"classpath_separator": string(os.PathListSeparator),
		"library_directory":   filepath.Join(configuration.GameDirectory, "libraries"),
		"user_properties":     "{}",
		"resolution_width":    "1280",
		"resolution_height":   "720",
	}
	if logFile := metadata.Logging.Client.File; logFile.URL != "" {
		logPath := filepath.Join(configuration.GameDirectory, "assets", "log_configs", safeComponent(logFile.ID))
		if err := fetch(ctx, logFile.URL, logPath, logFile.SHA1, logFile.Size, writer); err != nil {
			return err
		}
		variables["path"] = logPath
		variables["log_config"] = logPath
	}

	jvmArgs := jvmArguments(configuration, nativeDir)
	if len(metadata.Arguments.JVM) > 0 {
		parsed, err := expandArguments(metadata.Arguments.JVM, variables)
		if err != nil {
			return err
		}
		jvmArgs = append(jvmArgs, parsed...)
	} else {
		jvmArgs = append(jvmArgs, "-cp", variables["classpath"])
	}
	gameArgs := []string{}
	if len(metadata.Arguments.Game) > 0 {
		gameArgs, err = expandArguments(metadata.Arguments.Game, variables)
		if err != nil {
			return err
		}
	} else if metadata.MinecraftArguments != "" {
		for _, value := range strings.Fields(metadata.MinecraftArguments) {
			gameArgs = append(gameArgs, expand(value, variables))
		}
	} else {
		return fmt.Errorf("Minecraft %s has no supported game arguments", gameVersion)
	}

	logDirectory, err := os.UserCacheDir()
	if err != nil {
		return fmt.Errorf("locate launcher logs: %w", err)
	}
	logDirectory = filepath.Join(logDirectory, "Minty Launcher", "logs")
	if err := os.MkdirAll(logDirectory, 0o755); err != nil {
		return fmt.Errorf("create game log folder: %w", err)
	}
	logFile, err := os.OpenFile(filepath.Join(logDirectory, "minecraft.log"), os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o644)
	if err != nil {
		return fmt.Errorf("open game log: %w", err)
	}
	command := exec.Command(java, append(jvmArgs, append([]string{metadata.MainClass}, gameArgs...)...)...)
	command.Dir = instanceDir
	command.Stdout = logFile
	command.Stderr = logFile
	if err := command.Start(); err != nil {
		_ = logFile.Close()
		return fmt.Errorf("start Minecraft with Java: %w", err)
	}
	_ = writer.Send(protocol.Event{Type: "status", Message: fmt.Sprintf("Minecraft %s запущен (PID %d).", gameVersion, command.Process.Pid)})
	_ = writer.Send(protocol.Event{Type: "complete", Message: "Игра запущена."})
	return nil
}

func validateLaunchLoader(configuration settings.Settings) error {
	if !strings.EqualFold(configuration.ModLoader, "vanilla") {
		return fmt.Errorf("запуск %s пока не поддерживается: этот выпуск устанавливает только Vanilla", configuration.ModLoader)
	}
	return nil
}

func prepare(ctx context.Context, configuration settings.Settings, writer *protocol.Writer) (*versionMetadata, string, error) {
	if err := settings.Validate(configuration); err != nil {
		return nil, "", err
	}
	version := configuration.SelectedVersion
	client := &http.Client{Timeout: 10 * time.Minute}
	_ = writer.Send(protocol.Event{Type: "status", Message: "Получаем список версий Minecraft…"})
	manifest, err := fetchManifest(ctx, client, versionManifestURL)
	if err != nil {
		return nil, "", err
	}
	if version == "latest-release" {
		version = manifest.Latest.Release
	}
	if !versionIDPattern.MatchString(version) {
		return nil, "", fmt.Errorf("invalid Minecraft version ID")
	}
	var record *versionRecord
	for index := range manifest.Versions {
		if manifest.Versions[index].ID == version {
			record = &manifest.Versions[index]
			break
		}
	}
	if record == nil {
		return nil, "", fmt.Errorf("Minecraft version %s is not in the official manifest", version)
	}
	gameRoot := configuration.GameDirectory
	versionDirectory := filepath.Join(gameRoot, "versions", version)
	metadataPath := filepath.Join(versionDirectory, version+".json")
	if err := os.MkdirAll(versionDirectory, 0o755); err != nil {
		return nil, "", fmt.Errorf("create version folder: %w", err)
	}
	_ = writer.Send(protocol.Event{Type: "status", Message: "Загружаем метаданные Minecraft " + version})
	if err := fetch(ctx, record.URL, metadataPath, record.SHA1, 0, writer); err != nil {
		return nil, "", err
	}
	data, err := os.ReadFile(metadataPath)
	if err != nil {
		return nil, "", fmt.Errorf("read Minecraft metadata: %w", err)
	}
	var metadata versionMetadata
	if err := json.Unmarshal(data, &metadata); err != nil {
		return nil, "", fmt.Errorf("parse Minecraft metadata: %w", err)
	}
	if metadata.ID == "" || metadata.MainClass == "" || metadata.Downloads.Client.URL == "" {
		return nil, "", fmt.Errorf("Minecraft metadata is missing required launch information")
	}
	if err := fetch(ctx, metadata.Downloads.Client.URL, filepath.Join(versionDirectory, version+".jar"), metadata.Downloads.Client.SHA1, metadata.Downloads.Client.Size, writer); err != nil {
		return nil, "", err
	}
	librariesDirectory := filepath.Join(gameRoot, "libraries")
	var classpathArtifacts []artifact
	var nativeArtifacts []artifact
	for _, item := range metadata.Libraries {
		if !rulesAllow(item.Rules, false) {
			continue
		}
		libraryArtifact := item.Downloads.Artifact
		if libraryArtifact == nil {
			libraryArtifact = mavenArtifact(item)
		}
		if libraryArtifact != nil && libraryArtifact.URL != "" && libraryArtifact.Path != "" {
			path := filepath.Join(librariesDirectory, filepath.FromSlash(libraryArtifact.Path))
			if err := fetch(ctx, libraryArtifact.URL, path, libraryArtifact.SHA1, libraryArtifact.Size, writer); err != nil {
				return nil, "", err
			}
			classpathArtifacts = append(classpathArtifacts, *libraryArtifact)
		}
		for platform, classifier := range item.Natives {
			if platform != platformName() {
				continue
			}
			classifier = strings.ReplaceAll(classifier, "${arch}", architectureBits())
			if native, exists := item.Downloads.Classifiers[classifier]; exists && native.URL != "" && native.Path != "" {
				path := filepath.Join(librariesDirectory, filepath.FromSlash(native.Path))
				if err := fetch(ctx, native.URL, path, native.SHA1, native.Size, writer); err != nil {
					return nil, "", err
				}
				nativeArtifacts = append(nativeArtifacts, native)
			}
		}
	}
	_ = classpathArtifacts
	assets := metadata.AssetIndex
	if assets.URL != "" && assets.ID != "" {
		indexPath := filepath.Join(gameRoot, "assets", "indexes", safeComponent(assets.ID)+".json")
		if err := fetch(ctx, assets.URL, indexPath, assets.SHA1, assets.Size, writer); err != nil {
			return nil, "", err
		}
		indexData, err := os.ReadFile(indexPath)
		if err != nil {
			return nil, "", fmt.Errorf("read Minecraft asset index: %w", err)
		}
		var index assetManifest
		if err := json.Unmarshal(indexData, &index); err != nil {
			return nil, "", fmt.Errorf("parse Minecraft asset index: %w", err)
		}
		if err := fetchAssets(ctx, client, index, gameRoot, writer); err != nil {
			return nil, "", err
		}
	}
	nativeDirectory := filepath.Join(gameRoot, "natives", version)
	if err := os.MkdirAll(nativeDirectory, 0o755); err != nil {
		return nil, "", fmt.Errorf("create native libraries folder: %w", err)
	}
	for _, native := range nativeArtifacts {
		if err := extractNativeArchive(filepath.Join(librariesDirectory, filepath.FromSlash(native.Path)), nativeDirectory); err != nil {
			return nil, "", err
		}
	}
	instance := gameRoot
	if configuration.IsolatedProfiles {
		instance = filepath.Join(gameRoot, "instances", safeComponent(version))
	}
	if err := os.MkdirAll(instance, 0o755); err != nil {
		return nil, "", fmt.Errorf("create game instance folder: %w", err)
	}
	return &metadata, instance, nil
}

func fetch(ctx context.Context, url, destination, checksum string, size int64, writer *protocol.Writer) error {
	if url == "" {
		return nil
	}
	name := filepath.Base(destination)
	return download.Fetch(ctx, nil, url, destination, checksum, size, func(progress download.Progress) {
		fraction := 0.0
		if progress.Total > 0 {
			fraction = float64(progress.Downloaded) / float64(progress.Total)
		}
		_ = writer.Send(protocol.Event{
			Type: "download_progress", Message: name, Progress: fraction,
			Downloaded: progress.Downloaded, Total: progress.Total,
			BytesPerSecond: progress.BytesPerSecond, ETASeconds: progress.ETASeconds,
		})
	})
}

func jvmArguments(configuration settings.Settings, nativeDirectory string) []string {
	arguments := []string{"-Xms1G", fmt.Sprintf("-Xmx%dM", configuration.MaxMemoryMB)}
	if configuration.WeakPCMode {
		arguments = append(arguments, "-XX:+UseG1GC", "-XX:MaxGCPauseMillis=100")
	}
	return append(arguments, "-Djava.library.path="+nativeDirectory)
}

func fetchAssets(ctx context.Context, client *http.Client, index assetManifest, gameRoot string, writer *protocol.Writer) error {
	for _, item := range index.Objects {
		if len(item.Hash) != 40 {
			return fmt.Errorf("Minecraft asset manifest contains an invalid hash")
		}
		url := "https://resources.download.minecraft.net/" + item.Hash[:2] + "/" + item.Hash
		destination := filepath.Join(gameRoot, "assets", "objects", item.Hash[:2], item.Hash)
		if err := download.Fetch(ctx, client, url, destination, item.Hash, item.Size, func(progress download.Progress) {
			fraction := 0.0
			if progress.Total > 0 {
				fraction = float64(progress.Downloaded) / float64(progress.Total)
			}
			_ = writer.Send(protocol.Event{Type: "download_progress", Message: "Minecraft assets", Progress: fraction,
				Downloaded: progress.Downloaded, Total: progress.Total, BytesPerSecond: progress.BytesPerSecond, ETASeconds: progress.ETASeconds})
		}); err != nil {
			return fmt.Errorf("download Minecraft asset %s: %w", item.Hash, err)
		}
	}
	return nil
}

func mavenArtifact(item library) *artifact {
	coordinates, extension := item.Name, "jar"
	if split := strings.SplitN(item.Name, "@", 2); len(split) == 2 {
		coordinates, extension = split[0], split[1]
	}
	parts := strings.Split(coordinates, ":")
	if len(parts) < 3 || len(parts) > 4 {
		return nil
	}
	for _, part := range parts {
		if part == "" || strings.Contains(part, "..") || strings.ContainsAny(part, `/\`) {
			return nil
		}
	}
	group := strings.ReplaceAll(parts[0], ".", "/")
	classifier := ""
	if len(parts) == 4 {
		classifier = "-" + parts[3]
	}
	path := fmt.Sprintf("%s/%s/%s/%s-%s%s.%s", group, parts[1], parts[2], parts[1], parts[2], classifier, extension)
	base := item.URL
	if base == "" {
		base = "https://libraries.minecraft.net/"
	}
	return &artifact{Path: path, URL: strings.TrimRight(base, "/") + "/" + path}
}

func rulesAllow(rules []argumentRule, demo bool) bool {
	if len(rules) == 0 {
		return true
	}
	allowed := false
	for _, rule := range rules {
		matches := true
		if rule.OS != nil {
			if name := rule.OS["name"]; name != "" && name != platformName() {
				matches = false
			}
			if architecture := rule.OS["arch"]; architecture != "" && !architectureMatches(architecture) {
				matches = false
			}
		}
		for feature, expected := range rule.Features {
			actual := feature == "is_demo_user" && demo
			if actual != expected {
				matches = false
			}
		}
		if matches {
			allowed = rule.Action == "allow"
		}
	}
	return allowed
}

func architectureMatches(rule string) bool {
	rule = strings.ToLower(rule)
	if architectureBits() == "64" {
		return strings.Contains(rule, "64") || strings.Contains(rule, "amd64") || strings.Contains(rule, "aarch64")
	}
	return strings.Contains(rule, "x86") || strings.Contains(rule, "i386") || rule == "32"
}

func expandArguments(values []json.RawMessage, variables map[string]string) ([]string, error) {
	var result []string
	for _, raw := range values {
		var text string
		if err := json.Unmarshal(raw, &text); err == nil {
			result = append(result, expand(text, variables))
			continue
		}
		var conditional struct {
			Rules []argumentRule  `json:"rules"`
			Value json.RawMessage `json:"value"`
		}
		if err := json.Unmarshal(raw, &conditional); err != nil {
			return nil, fmt.Errorf("parse Minecraft launch argument: %w", err)
		}
		if !rulesAllow(conditional.Rules, false) {
			continue
		}
		var textValue string
		if err := json.Unmarshal(conditional.Value, &textValue); err == nil {
			result = append(result, expand(textValue, variables))
			continue
		}
		var textValues []string
		if err := json.Unmarshal(conditional.Value, &textValues); err != nil {
			return nil, fmt.Errorf("unsupported Minecraft launch argument value")
		}
		for _, value := range textValues {
			result = append(result, expand(value, variables))
		}
	}
	return result, nil
}

func expand(value string, variables map[string]string) string {
	for key, replacement := range variables {
		value = strings.ReplaceAll(value, "${"+key+"}", replacement)
	}
	return value
}

func platformName() string {
	switch runtime.GOOS {
	case "windows":
		return "windows"
	case "darwin":
		return "osx"
	default:
		return "linux"
	}
}

func architectureBits() string {
	if runtime.GOARCH == "386" || runtime.GOARCH == "arm" {
		return "32"
	}
	return "64"
}

func safeComponent(value string) string {
	var result strings.Builder
	for _, character := range value {
		if character >= 'A' && character <= 'Z' || character >= 'a' && character <= 'z' || character >= '0' && character <= '9' || character == '-' || character == '_' || character == '.' {
			result.WriteRune(character)
		} else {
			result.WriteByte('_')
		}
	}
	name := strings.Trim(result.String(), ".")
	if name == "" {
		return "minecraft"
	}
	return name
}

func extractNativeArchive(archivePath, destination string) error {
	archive, err := zip.OpenReader(archivePath)
	if err != nil {
		return fmt.Errorf("open native library archive: %w", err)
	}
	defer archive.Close()
	root, err := filepath.Abs(destination)
	if err != nil {
		return err
	}
	for _, file := range archive.File {
		if strings.HasPrefix(file.Name, "META-INF/") || file.FileInfo().IsDir() {
			continue
		}
		target := filepath.Join(root, filepath.FromSlash(file.Name))
		absolute, err := filepath.Abs(target)
		if err != nil {
			return err
		}
		relative, err := filepath.Rel(root, absolute)
		if err != nil || relative == ".." || strings.HasPrefix(relative, ".."+string(filepath.Separator)) || filepath.IsAbs(relative) {
			return fmt.Errorf("unsafe native archive path")
		}
		if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
			return err
		}
		input, err := file.Open()
		if err != nil {
			return err
		}
		output, err := os.OpenFile(target, os.O_CREATE|os.O_TRUNC|os.O_WRONLY, 0o755)
		if err != nil {
			_ = input.Close()
			return err
		}
		_, copyErr := io.Copy(output, input)
		closeOutputErr := output.Close()
		closeInputErr := input.Close()
		if copyErr != nil {
			return copyErr
		}
		if closeOutputErr != nil {
			return closeOutputErr
		}
		if closeInputErr != nil {
			return closeInputErr
		}
	}
	return nil
}

func extractNatives(archivePath, destination string) error {
	return extractNativeArchive(archivePath, destination)
}

func resolveJava(customPath string) (string, error) {
	if customPath != "" {
		path := customPath
		if info, err := os.Stat(path); err == nil && info.IsDir() {
			path = filepath.Join(path, "bin", javaExecutable())
		}
		if filepath.Base(path) == "java.exe" {
			javaw := filepath.Join(filepath.Dir(path), "javaw.exe")
			if _, err := os.Stat(javaw); err == nil {
				path = javaw
			}
		}
		if _, err := os.Stat(path); err != nil {
			return "", fmt.Errorf("configured Java executable not found: %s", path)
		}
		return path, nil
	}
	for _, candidate := range []string{"javaw.exe", "java.exe", "java"} {
		if path, err := exec.LookPath(candidate); err == nil {
			return path, nil
		}
	}
	return "", fmt.Errorf("Java is not installed or not on PATH; select a Java executable in launcher settings")
}

func javaExecutable() string {
	if runtime.GOOS == "windows" {
		return "javaw.exe"
	}
	return "java"
}

func checkJava(ctx context.Context, java string, requiredMajor int) error {
	probe := java
	if runtime.GOOS == "windows" && strings.EqualFold(filepath.Base(probe), "javaw.exe") {
		consoleJava := filepath.Join(filepath.Dir(probe), "java.exe")
		if _, err := os.Stat(consoleJava); err == nil {
			probe = consoleJava
		}
	}
	command := exec.CommandContext(ctx, probe, "-version")
	output, err := command.CombinedOutput()
	if err != nil {
		return fmt.Errorf("could not query Java version: %w", err)
	}
	actual, err := parseJavaMajor(string(output))
	if err != nil {
		return err
	}
	if requiredMajor > 0 && actual < requiredMajor {
		return fmt.Errorf("Minecraft requires Java %d or newer; found Java %d", requiredMajor, actual)
	}
	return nil
}

func parseJavaMajor(versionText string) (int, error) {
	version := regexp.MustCompile(`(?:version|openjdk)\s+(?:version\s+)?["']?([0-9]+(?:\.[0-9]+)*)`).FindStringSubmatch(versionText)
	if len(version) < 2 {
		return 0, fmt.Errorf("could not parse Java version from %q", strings.TrimSpace(versionText))
	}
	parts := strings.Split(version[1], ".")
	majorIndex := 0
	if parts[0] == "1" && len(parts) > 1 {
		majorIndex = 1
	}
	var major int
	if _, err := fmt.Sscanf(parts[majorIndex], "%d", &major); err != nil {
		return 0, fmt.Errorf("invalid Java version %q", version[1])
	}
	return major, nil
}

func requiredJava(version string) int {
	parts := strings.Split(version, ".")
	if len(parts) < 2 {
		return 8
	}
	var major, minor int
	_, _ = fmt.Sscanf(parts[0], "%d", &major)
	_, _ = fmt.Sscanf(parts[1], "%d", &minor)
	if major != 1 {
		return major
	}
	if minor <= 16 {
		return 8
	}
	if minor == 17 {
		return 16
	}
	if minor <= 20 {
		return 17
	}
	return 21
}

func offlineUUID(username string) string {
	digest := md5.Sum([]byte("OfflinePlayer:" + username))
	digest[6] = (digest[6] & 0x0f) | 0x30
	digest[8] = (digest[8] & 0x3f) | 0x80
	value := hex.EncodeToString(digest[:])
	return value[:8] + "-" + value[8:12] + "-" + value[12:16] + "-" + value[16:20] + "-" + value[20:]
}
