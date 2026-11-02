package minecraft

import (
	"context"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestFetchVersionsReturnsLatestAndValidEntries(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		writer.Header().Set("Content-Type", "application/json")
		_, _ = writer.Write([]byte(`{"latest":{"release":"1.21.4","snapshot":"25w01a"},"versions":[{"id":"1.21.4","type":"release","releaseTime":"2025-01-01T00:00:00Z","url":"https://example.test/release.json"},{"id":"bad","type":"snapshot"}]}`))
	}))
	defer server.Close()

	versions, latest, err := fetchVersions(context.Background(), server.Client(), server.URL)
	if err != nil {
		t.Fatalf("fetchVersions() error = %v", err)
	}
	if latest != "1.21.4" || len(versions) != 1 || versions[0].ID != "1.21.4" {
		t.Fatalf("unexpected result: latest=%q versions=%+v", latest, versions)
	}
}
