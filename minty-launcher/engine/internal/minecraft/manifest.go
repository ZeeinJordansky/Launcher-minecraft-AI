package minecraft

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"time"

	"minty-engine/internal/protocol"
)

const versionManifestURL = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json"

type manifest struct {
	Latest struct {
		Release string `json:"release"`
		Snapshot string `json:"snapshot"`
	} `json:"latest"`
	Versions []versionRecord `json:"versions"`
}

type versionRecord struct {
	ID          string `json:"id"`
	Type        string `json:"type"`
	URL         string `json:"url"`
	SHA1        string `json:"sha1"`
	ReleaseTime string `json:"releaseTime"`
}

func ListVersions(ctx context.Context, client *http.Client) ([]protocol.Version, string, error) {
	if client == nil {
		client = &http.Client{Timeout: 45 * time.Second}
	}
	return fetchVersions(ctx, client, versionManifestURL)
}

func fetchVersions(ctx context.Context, client *http.Client, endpoint string) ([]protocol.Version, string, error) {
	data, err := fetchManifest(ctx, client, endpoint)
	if err != nil {
		return nil, "", err
	}
	if data.Latest.Release == "" || len(data.Versions) == 0 {
		return nil, "", fmt.Errorf("Minecraft version manifest is incomplete")
	}
	versions := make([]protocol.Version, 0, len(data.Versions))
	for _, version := range data.Versions {
		if version.ID == "" || version.URL == "" {
			continue
		}
		versions = append(versions, protocol.Version{ID: version.ID, Type: version.Type, ReleaseTime: version.ReleaseTime})
	}
	return versions, data.Latest.Release, nil
}

func fetchManifest(ctx context.Context, client *http.Client, endpoint string) (manifest, error) {
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, endpoint, nil)
	if err != nil {
		return manifest{}, fmt.Errorf("create version request: %w", err)
	}
	request.Header.Set("User-Agent", "MintyLauncher/2.0.0-global")
	response, err := client.Do(request)
	if err != nil {
		return manifest{}, fmt.Errorf("fetch Minecraft version manifest: %w", err)
	}
	defer response.Body.Close()
	if response.StatusCode < 200 || response.StatusCode >= 300 {
		return manifest{}, fmt.Errorf("fetch Minecraft version manifest: HTTP %s", response.Status)
	}

	var data manifest
	if err := json.NewDecoder(response.Body).Decode(&data); err != nil {
		return manifest{}, fmt.Errorf("decode Minecraft version manifest: %w", err)
	}
	return data, nil
}
