package runtime

import (
	"archive/zip"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"time"

	"minty-engine/internal/protocol"
)

type adoptiumAsset struct {
	Binary struct {
		Package struct {
			Link     string `json:"link"`
			Checksum string `json:"checksum"`
		} `json:"package"`
	} `json:"binary"`
}

func Ensure(ctx context.Context, customPath string, requiredMajor int, runtimeRoot string, writer *protocol.Writer) (string, error) {
	if customPath != "" {
		path := customPath
		if info, err := os.Stat(path); err == nil && info.IsDir() {
			path = filepath.Join(path, "bin", executableName())
		}
		if filepath.Base(path) == "java.exe" && runtime.GOOS == "windows" {
			javaw := filepath.Join(filepath.Dir(path), "javaw.exe")
			if _, err := os.Stat(javaw); err == nil {
				path = javaw
			}
		}
		if err := checkVersion(ctx, path, requiredMajor); err != nil {
			return "", fmt.Errorf("указанная Java не подходит: %w", err)
		}
		return path, nil
	}

	for _, name := range []string{"javaw.exe", "java.exe", "java"} {
		path, err := exec.LookPath(name)
		if err == nil && checkVersion(ctx, path, requiredMajor) == nil {
			return path, nil
		}
	}
	managedPath := filepath.Join(runtimeRoot, fmt.Sprintf("temurin-%d", requiredMajor))
	if path := findExecutable(managedPath); path != "" && checkVersion(ctx, path, requiredMajor) == nil {
		return path, nil
	}
	if err := installTemurin(ctx, requiredMajor, runtimeRoot, writer); err != nil {
		return "", err
	}
	path := findExecutable(managedPath)
	if path == "" {
		return "", fmt.Errorf("Java runtime downloaded but executable was not found")
	}
	if err := checkVersion(ctx, path, requiredMajor); err != nil {
		return "", err
	}
	return path, nil
}

func installTemurin(ctx context.Context, major int, runtimeRoot string, writer *protocol.Writer) error {
	if major < 8 || major > 99 {
		return fmt.Errorf("unsupported Java major version %d", major)
	}
	if runtime.GOOS != "windows" {
		return fmt.Errorf("automatic Java runtime installation is currently supported on Windows only; configure java manually")
	}
	architecture := "x64"
	if runtime.GOARCH == "arm64" {
		architecture = "aarch64"
	}
	endpoint := fmt.Sprintf("https://api.adoptium.net/v3/assets/latest/%d/hotspot?architecture=%s&heap_size=normal&image_type=jre&os=windows&vendor=eclipse", major, architecture)
	client := &http.Client{Timeout: 10 * time.Minute}
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, endpoint, nil)
	if err != nil {
		return fmt.Errorf("create Temurin request: %w", err)
	}
	response, err := client.Do(request)
	if err != nil {
		return fmt.Errorf("query Temurin runtime: %w", err)
	}
	defer response.Body.Close()
	if response.StatusCode < 200 || response.StatusCode >= 300 {
		return fmt.Errorf("Temurin API returned HTTP %s", response.Status)
	}
	var assets []adoptiumAsset
	if err := json.NewDecoder(response.Body).Decode(&assets); err != nil {
		return fmt.Errorf("decode Temurin runtime metadata: %w", err)
	}
	if len(assets) == 0 || !strings.HasPrefix(assets[0].Binary.Package.Link, "https://") || len(assets[0].Binary.Package.Checksum) != 64 {
		return fmt.Errorf("Temurin API did not provide an HTTPS JRE archive and SHA-256 checksum")
	}

	_ = writer.Send(protocol.Event{Type: "status", Message: fmt.Sprintf("Загружаем Java %d (Eclipse Temurin)…", major)})
	archivePath := filepath.Join(runtimeRoot, fmt.Sprintf("temurin-%d.zip", major))
	if err := os.MkdirAll(runtimeRoot, 0o755); err != nil {
		return fmt.Errorf("create Java runtime folder: %w", err)
	}
	if err := downloadRuntime(ctx, client, assets[0].Binary.Package.Link, archivePath, assets[0].Binary.Package.Checksum, writer); err != nil {
		return err
	}
	temporaryDirectory := filepath.Join(runtimeRoot, fmt.Sprintf(".temurin-%d-installing", major))
	_ = os.RemoveAll(temporaryDirectory)
	if err := extractSafely(archivePath, temporaryDirectory); err != nil {
		_ = os.RemoveAll(temporaryDirectory)
		return err
	}
	destination := filepath.Join(runtimeRoot, fmt.Sprintf("temurin-%d", major))
	_ = os.RemoveAll(destination)
	if err := os.Rename(temporaryDirectory, destination); err != nil {
		return fmt.Errorf("activate Java runtime: %w", err)
	}
	_ = os.Remove(archivePath)
	return nil
}

func downloadRuntime(ctx context.Context, client *http.Client, url, destination, expectedSHA256 string, writer *protocol.Writer) error {
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return fmt.Errorf("create Java download request: %w", err)
	}
	response, err := client.Do(request)
	if err != nil {
		return fmt.Errorf("download Java runtime: %w", err)
	}
	defer response.Body.Close()
	if response.StatusCode < 200 || response.StatusCode >= 300 {
		return fmt.Errorf("Java runtime download returned HTTP %s", response.Status)
	}
	if response.ContentLength <= 0 || response.ContentLength > 2*1024*1024*1024 {
		return fmt.Errorf("Java runtime archive has an invalid size")
	}
	temporary, err := os.CreateTemp(filepath.Dir(destination), ".temurin-*.partial")
	if err != nil {
		return fmt.Errorf("create temporary Java archive: %w", err)
	}
	temporaryPath := temporary.Name()
	defer os.Remove(temporaryPath)
	hash := sha256.New()
	started := time.Now()
	var downloaded int64
	buffer := make([]byte, 128*1024)
	for {
		count, readErr := response.Body.Read(buffer)
		if count > 0 {
			if _, err := temporary.Write(buffer[:count]); err != nil {
				_ = temporary.Close()
				return fmt.Errorf("write Java runtime archive: %w", err)
			}
			_, _ = hash.Write(buffer[:count])
			downloaded += int64(count)
			elapsed := time.Since(started).Seconds()
			if elapsed < 0.001 {
				elapsed = 0.001
			}
			speed := float64(downloaded) / elapsed
			remaining := response.ContentLength - downloaded
			eta := int64(-1)
			if speed > 0 {
				eta = int64(float64(remaining) / speed)
			}
			_ = writer.Send(protocol.Event{Type: "download_progress", Message: "Java Runtime", Progress: float64(downloaded) / float64(response.ContentLength), Downloaded: downloaded, Total: response.ContentLength, BytesPerSecond: speed, ETASeconds: eta})
		}
		if readErr == io.EOF {
			break
		}
		if readErr != nil {
			_ = temporary.Close()
			return fmt.Errorf("read Java runtime archive: %w", readErr)
		}
	}
	if downloaded != response.ContentLength || !strings.EqualFold(hex.EncodeToString(hash.Sum(nil)), expectedSHA256) {
		_ = temporary.Close()
		return fmt.Errorf("Java runtime archive failed size or SHA-256 verification")
	}
	if err := temporary.Sync(); err != nil {
		_ = temporary.Close()
		return fmt.Errorf("flush Java archive: %w", err)
	}
	if err := temporary.Close(); err != nil {
		return fmt.Errorf("close Java archive: %w", err)
	}
	if err := os.Rename(temporaryPath, destination); err != nil {
		return fmt.Errorf("stage Java runtime archive: %w", err)
	}
	return nil
}

func extractSafely(archivePath, destination string) error {
	archive, err := zip.OpenReader(archivePath)
	if err != nil {
		return fmt.Errorf("open Java runtime archive: %w", err)
	}
	defer archive.Close()
	root, err := filepath.Abs(destination)
	if err != nil {
		return err
	}
	if err := os.MkdirAll(root, 0o755); err != nil {
		return fmt.Errorf("create temporary Java directory: %w", err)
	}
	var expanded int64
	for _, file := range archive.File {
		if file.FileInfo().IsDir() || file.Mode()&os.ModeSymlink != 0 {
			continue
		}
		relative := filepath.Clean(filepath.FromSlash(file.Name))
		if filepath.IsAbs(relative) || relative == "." || relative == ".." || strings.HasPrefix(relative, ".."+string(filepath.Separator)) {
			return fmt.Errorf("Java runtime archive contains an unsafe path")
		}
		expanded += int64(file.UncompressedSize64)
		if expanded > 2*1024*1024*1024 {
			return fmt.Errorf("Java runtime archive exceeds the extraction limit")
		}
		target := filepath.Join(root, relative)
		absolute, err := filepath.Abs(target)
		if err != nil {
			return err
		}
		if !strings.HasPrefix(absolute, root+string(filepath.Separator)) {
			return fmt.Errorf("Java runtime archive path escapes destination")
		}
		if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
			return err
		}
		input, err := file.Open()
		if err != nil {
			return err
		}
		output, err := os.OpenFile(target, os.O_CREATE|os.O_TRUNC|os.O_WRONLY, file.Mode().Perm())
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

func findExecutable(root string) string {
	var found string
	_ = filepath.WalkDir(root, func(path string, entry os.DirEntry, err error) error {
		if err != nil || found != "" {
			return filepath.SkipDir
		}
		if entry.IsDir() {
			return nil
		}
		name := strings.ToLower(entry.Name())
		if name == "javaw.exe" || name == "java" {
			found = path
		}
		return nil
	})
	return found
}

func checkVersion(ctx context.Context, path string, requiredMajor int) error {
	probe := path
	if runtime.GOOS == "windows" && strings.EqualFold(filepath.Base(probe), "javaw.exe") {
		console := filepath.Join(filepath.Dir(probe), "java.exe")
		if _, err := os.Stat(console); err == nil {
			probe = console
		}
	}
	command := exec.CommandContext(ctx, probe, "-version")
	output, err := command.CombinedOutput()
	if err != nil {
		return fmt.Errorf("query Java version: %w", err)
	}
	major, err := parseMajor(string(output))
	if err != nil {
		return err
	}
	if requiredMajor <= 16 && major != requiredMajor || requiredMajor > 16 && major < requiredMajor {
		return fmt.Errorf("Minecraft needs Java %d or newer, found Java %d", requiredMajor, major)
	}
	return nil
}

func parseMajor(output string) (int, error) {
	fields := strings.Fields(output)
	for index, field := range fields {
		if field != "version" && field != "openjdk" {
			continue
		}
		if index+1 >= len(fields) {
			continue
		}
		value := strings.Trim(fields[index+1], `"'`)
		parts := strings.Split(value, ".")
		majorIndex := 0
		if parts[0] == "1" && len(parts) > 1 {
			majorIndex = 1
		}
		var major int
		if _, err := fmt.Sscanf(parts[majorIndex], "%d", &major); err == nil {
			return major, nil
		}
	}
	return 0, fmt.Errorf("could not parse Java version")
}

func executableName() string {
	if runtime.GOOS == "windows" {
		return "javaw.exe"
	}
	return "java"
}
