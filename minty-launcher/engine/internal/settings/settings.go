package settings

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strings"
)

var versionIDPattern = regexp.MustCompile(`^[A-Za-z0-9._-]+$`)

type Settings struct {
	GameDirectory    string `json:"gameDirectory"`
	SelectedVersion  string `json:"selectedVersion"`
	Username         string `json:"username"`
	JavaPath         string `json:"javaPath"`
	MaxMemoryMB      int    `json:"maxMemoryMb"`
	WeakPCMode       bool   `json:"weakPcMode"`
	EnableAnimations bool   `json:"enableAnimations"`
	ModLoader        string `json:"modLoader"`
	IsolatedProfiles bool   `json:"isolatedProfiles"`
}

func Defaults() Settings {
	home, err := os.UserHomeDir()
	if err != nil {
		home = "."
	}
	return Settings{
		GameDirectory:    filepath.Join(home, ".minecraft"),
		SelectedVersion:  "latest-release",
		Username:         "Player",
		MaxMemoryMB:      4096,
		EnableAnimations: true,
		ModLoader:        "Vanilla",
	}
}

func Path() (string, error) {
	root, err := os.UserConfigDir()
	if err != nil {
		return "", fmt.Errorf("locate user config directory: %w", err)
	}
	return filepath.Join(root, "Minty Launcher", "settings.json"), nil
}

func Load() (Settings, error) {
	path, err := Path()
	if err != nil {
		return Settings{}, err
	}
	data, err := os.ReadFile(path)
	if errors.Is(err, os.ErrNotExist) {
		return Defaults(), nil
	}
	if err != nil {
		return Settings{}, fmt.Errorf("read settings: %w", err)
	}
	settings := Defaults()
	if err := json.Unmarshal(data, &settings); err != nil {
		return Settings{}, fmt.Errorf("parse settings: %w", err)
	}
	return settings, nil
}

func Save(settings Settings) error {
	if err := Validate(settings); err != nil {
		return err
	}
	path, err := Path()
	if err != nil {
		return err
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return fmt.Errorf("create settings directory: %w", err)
	}
	data, err := json.MarshalIndent(settings, "", "  ")
	if err != nil {
		return fmt.Errorf("encode settings: %w", err)
	}
	temporary, err := os.CreateTemp(filepath.Dir(path), ".settings-*.tmp")
	if err != nil {
		return fmt.Errorf("create temporary settings: %w", err)
	}
	name := temporary.Name()
	defer os.Remove(name)
	if _, err := temporary.Write(data); err != nil {
		_ = temporary.Close()
		return fmt.Errorf("write temporary settings: %w", err)
	}
	if err := temporary.Sync(); err != nil {
		_ = temporary.Close()
		return fmt.Errorf("flush temporary settings: %w", err)
	}
	if err := temporary.Close(); err != nil {
		return fmt.Errorf("close temporary settings: %w", err)
	}
	if err := os.Rename(name, path); err != nil {
		return fmt.Errorf("replace settings: %w", err)
	}
	return nil
}

func Validate(settings Settings) error {
	if strings.TrimSpace(settings.GameDirectory) == "" || !filepath.IsAbs(settings.GameDirectory) {
		return fmt.Errorf("game directory must be an absolute path")
	}
	username := strings.TrimSpace(settings.Username)
	if len(username) < 3 || len(username) > 16 {
		return fmt.Errorf("offline username must contain 3 to 16 characters")
	}
	for _, character := range username {
		if !(character >= 'A' && character <= 'Z' || character >= 'a' && character <= 'z' || character >= '0' && character <= '9' || character == '_') {
			return fmt.Errorf("offline username may contain only ASCII letters, numbers and underscores")
		}
	}
	if settings.MaxMemoryMB < 1024 || settings.MaxMemoryMB > 65536 {
		return fmt.Errorf("memory limit must be between 1024 and 65536 MB")
	}
	if strings.TrimSpace(settings.SelectedVersion) == "" {
		return fmt.Errorf("Minecraft version cannot be empty")
	}
	if !versionIDPattern.MatchString(settings.SelectedVersion) || settings.SelectedVersion == "." || settings.SelectedVersion == ".." {
		return fmt.Errorf("Minecraft version contains invalid characters")
	}
	if settings.ModLoader == "" {
		return fmt.Errorf("mod loader cannot be empty")
	}
	return nil
}

func ProfileDirectory(settings Settings) (string, error) {
	if err := Validate(settings); err != nil {
		return "", err
	}
	if settings.IsolatedProfiles {
		return filepath.Join(settings.GameDirectory, "instances", settings.SelectedVersion), nil
	}
	return settings.GameDirectory, nil
}
