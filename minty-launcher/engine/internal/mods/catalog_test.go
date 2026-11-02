package mods

import (
	"bytes"
	"context"
	"crypto/sha1"
	"encoding/hex"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"minty-engine/internal/protocol"
)

func TestSearchBuildsCompatibilityFacets(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		facets := request.URL.Query().Get("facets")
		if !strings.Contains(facets, "project_type:mod") || !strings.Contains(facets, "versions:1.21.4") || !strings.Contains(facets, "categories:fabric") {
			t.Errorf("unexpected Modrinth facets: %s", facets)
		}
		writer.Header().Set("Content-Type", "application/json")
		_, _ = writer.Write([]byte(`{"hits":[{"slug":"sodium","title":"Sodium","author":"JellySquid","downloads":42,"project_type":"mod"}]}`))
	}))
	defer server.Close()

	projects, err := searchAt(context.Background(), server.Client(), server.URL, "sodium", "1.21.4", "Fabric", "mod", "downloads")
	if err != nil {
		t.Fatalf("searchAt() error = %v", err)
	}
	if len(projects) != 1 || projects[0].Slug != "sodium" {
		t.Fatalf("unexpected projects: %+v", projects)
	}
}

func TestInstallDownloadsCompatibleVerifiedJar(t *testing.T) {
	payload := []byte("valid mod fixture")
	checksum := sha1.Sum(payload)
	var server *httptest.Server
	server = httptest.NewTLSServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		switch request.URL.Path {
		case "/v2/project/example-mod/version":
			writer.Header().Set("Content-Type", "application/json")
			_ = json.NewEncoder(writer).Encode([]projectVersion{{
				GameVersions: []string{"1.21.4"},
				Loaders:      []string{"fabric"},
				Files: []projectFile{{
					Filename: "example-mod.jar", URL: server.URL + "/file.jar", Primary: true,
					Size: int64(len(payload)), Hashes: map[string]string{"sha1": hex.EncodeToString(checksum[:])},
				}},
			}})
		case "/file.jar":
			_, _ = writer.Write(payload)
		default:
			http.NotFound(writer, request)
		}
	}))
	defer server.Close()

	var events bytes.Buffer
	path, err := installAt(context.Background(), server.Client(), server.URL+"/v2", "example-mod", "1.21.4", "Fabric", "mod", t.TempDir(), protocol.NewWriter(&events))
	if err != nil {
		t.Fatalf("installAt() error = %v", err)
	}
	if filepath.Base(path) != "example-mod.jar" || !strings.Contains(filepath.ToSlash(path), "/mods/") {
		t.Fatalf("unexpected installation path: %s", path)
	}
	contents, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read installed mod: %v", err)
	}
	if !bytes.Equal(contents, payload) {
		t.Fatal("installed mod does not match verified payload")
	}
	lines := bytes.Split(bytes.TrimSpace(events.Bytes()), []byte("\n"))
	var event protocol.Event
	if err := json.Unmarshal(lines[len(lines)-1], &event); err != nil {
		t.Fatalf("progress event is not valid JSON: %v", err)
	}
	if event.Type != "download_progress" || event.Total != int64(len(payload)) {
		t.Fatalf("unexpected download event: %+v", event)
	}
}
