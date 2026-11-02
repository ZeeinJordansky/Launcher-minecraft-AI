package runtime

import (
	"archive/zip"
	"os"
	"path/filepath"
	"testing"
)

func TestParseJavaMajorVersions(t *testing.T) {
	cases := []struct {
		output string
		want   int
	}{
		{`java version "1.8.0_412"`, 8},
		{`openjdk version "17.0.12" 2024-07-16`, 17},
		{`openjdk version "21.0.2" 2024-01-16`, 21},
	}
	for _, test := range cases {
		got, err := parseMajor(test.output)
		if err != nil || got != test.want {
			t.Errorf("parseMajor(%q) = %d, %v; want %d", test.output, got, err, test.want)
		}
	}
}

func TestExtractSafelyRejectsZipSlip(t *testing.T) {
	root := t.TempDir()
	archivePath := filepath.Join(root, "runtime.zip")
	file, err := os.Create(archivePath)
	if err != nil {
		t.Fatal(err)
	}
	archive := zip.NewWriter(file)
	entry, err := archive.Create("../../outside.txt")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := entry.Write([]byte("outside")); err != nil {
		t.Fatal(err)
	}
	if err := archive.Close(); err != nil {
		t.Fatal(err)
	}
	if err := file.Close(); err != nil {
		t.Fatal(err)
	}

	destination := filepath.Join(root, "runtime")
	if err := extractSafely(archivePath, destination); err == nil {
		t.Fatal("extractSafely() accepted a path traversal entry")
	}
	if _, err := os.Stat(filepath.Join(root, "outside.txt")); !os.IsNotExist(err) {
		t.Fatalf("archive wrote outside the extraction folder: %v", err)
	}
}
