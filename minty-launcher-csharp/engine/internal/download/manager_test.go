package download

import (
	"context"
	"crypto/sha1"
	"encoding/hex"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestFetchVerifiesAndPublishesDownload(t *testing.T) {
	payload := strings.Repeat("minty", 4096)
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		_, _ = writer.Write([]byte(payload))
	}))
	defer server.Close()

	hash := sha1.Sum([]byte(payload))
	destination := filepath.Join(t.TempDir(), "versions", "client.jar")
	var last Progress
	err := Fetch(context.Background(), server.Client(), server.URL, destination, hex.EncodeToString(hash[:]), int64(len(payload)), func(progress Progress) {
		last = progress
	})
	if err != nil {
		t.Fatalf("Fetch() error = %v", err)
	}
	contents, err := os.ReadFile(destination)
	if err != nil {
		t.Fatalf("read downloaded file: %v", err)
	}
	if string(contents) != payload {
		t.Fatal("downloaded file contents do not match response")
	}
	if last.Downloaded != int64(len(payload)) || last.Total != int64(len(payload)) {
		t.Fatalf("unexpected final progress: %+v", last)
	}
}

func TestFetchRejectsChecksumMismatch(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		_, _ = writer.Write([]byte("corrupt"))
	}))
	defer server.Close()

	destination := filepath.Join(t.TempDir(), "client.jar")
	err := Fetch(context.Background(), server.Client(), server.URL, destination, strings.Repeat("0", 40), 7, nil)
	if err == nil {
		t.Fatal("Fetch() accepted a download with an invalid SHA-1")
	}
	if _, statErr := os.Stat(destination); !os.IsNotExist(statErr) {
		t.Fatalf("invalid download was published: stat error = %v", statErr)
	}
}
