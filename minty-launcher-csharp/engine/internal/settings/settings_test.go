package settings

import (
	"path/filepath"
	"testing"
)

func TestDefaultsAreValid(t *testing.T) {
	if err := Validate(Defaults()); err != nil {
		t.Fatalf("default settings are invalid: %v", err)
	}
}

func TestValidateRejectsUnsafeUsernamesAndMemory(t *testing.T) {
	settings := Defaults()
	settings.Username = "Player One"
	if err := Validate(settings); err == nil {
		t.Fatal("Validate() accepted a username containing spaces")
	}

	settings = Defaults()
	settings.MaxMemoryMB = 512
	if err := Validate(settings); err == nil {
		t.Fatal("Validate() accepted a memory limit below 1024 MB")
	}
}

func TestProfileDirectoryRespectsIsolatedProfiles(t *testing.T) {
	configuration := Defaults()
	configuration.GameDirectory = filepath.Join(string(filepath.Separator), "games", "minecraft")
	configuration.SelectedVersion = "1.21.4"

	directory, err := ProfileDirectory(configuration)
	if err != nil {
		t.Fatalf("ProfileDirectory() error = %v", err)
	}
	if directory != configuration.GameDirectory {
		t.Fatalf("shared profile directory = %q, want %q", directory, configuration.GameDirectory)
	}

	configuration.IsolatedProfiles = true
	directory, err = ProfileDirectory(configuration)
	if err != nil {
		t.Fatalf("ProfileDirectory() error = %v", err)
	}
	if want := filepath.Join(configuration.GameDirectory, "instances", configuration.SelectedVersion); directory != want {
		t.Fatalf("isolated profile directory = %q, want %q", directory, want)
	}
}

func TestValidateRejectsUnsafeVersionDirectory(t *testing.T) {
	configuration := Defaults()
	configuration.SelectedVersion = "../outside"
	if err := Validate(configuration); err == nil {
		t.Fatal("Validate() accepted a version that escapes its profile directory")
	}
}
