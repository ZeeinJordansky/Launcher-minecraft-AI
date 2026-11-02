package mods

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/url"
	"path/filepath"
	"strings"
	"time"

	"minty-engine/internal/download"
	"minty-engine/internal/protocol"
)

const apiURL = "https://api.modrinth.com/v2"

type searchResponse struct {
	Hits []project `json:"hits"`
}

type project struct {
	Slug        string   `json:"slug"`
	Title       string   `json:"title"`
	Description string   `json:"description"`
	Author      string   `json:"author"`
	Downloads   int64    `json:"downloads"`
	IconURL     string   `json:"icon_url"`
	ProjectType string   `json:"project_type"`
	Categories  []string `json:"categories"`
}

type projectFile struct {
	Filename string            `json:"filename"`
	URL      string            `json:"url"`
	Primary  bool              `json:"primary"`
	Size     int64             `json:"size"`
	Hashes   map[string]string `json:"hashes"`
}

type projectVersion struct {
	Files        []projectFile `json:"files"`
	GameVersions []string      `json:"game_versions"`
	Loaders      []string      `json:"loaders"`
}

func Search(ctx context.Context, client *http.Client, query, gameVersion, loader, category, sort string) ([]protocol.Project, error) {
	return searchAt(ctx, client, apiURL, query, gameVersion, loader, category, sort)
}

func searchAt(ctx context.Context, client *http.Client, baseURL, query, gameVersion, loader, category, sort string) ([]protocol.Project, error) {
	if client == nil {
		client = &http.Client{Timeout: 45 * time.Second}
	}
	kind := projectType(category)
	facets := [][]string{{"project_type:" + kind}}
	if gameVersion != "" && gameVersion != "latest-release" {
		facets = append(facets, []string{"versions:" + gameVersion})
	}
	if facet := loaderFacet(loader); facet != "" && kind == "mod" {
		facets = append(facets, []string{"categories:" + facet})
	}
	encodedFacets, err := json.Marshal(facets)
	if err != nil {
		return nil, fmt.Errorf("encode Modrinth filters: %w", err)
	}
	index := sort
	if index != "relevance" && index != "downloads" && index != "follows" && index != "newest" && index != "updated" {
		index = "downloads"
	}
	parameters := url.Values{}
	parameters.Set("query", query)
	parameters.Set("limit", "30")
	parameters.Set("index", index)
	parameters.Set("facets", string(encodedFacets))
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, strings.TrimRight(baseURL, "/")+"/search?"+parameters.Encode(), nil)
	if err != nil {
		return nil, fmt.Errorf("create Modrinth search: %w", err)
	}
	request.Header.Set("User-Agent", "MintyLauncher/2.0.0-global (https://github.com/ZeeinJordansky/Launcher-minecraft-AI)")
	response, err := client.Do(request)
	if err != nil {
		return nil, fmt.Errorf("search Modrinth: %w", err)
	}
	defer response.Body.Close()
	if response.StatusCode < 200 || response.StatusCode >= 300 {
		return nil, fmt.Errorf("Modrinth search returned HTTP %s", response.Status)
	}
	var result searchResponse
	if err := json.NewDecoder(response.Body).Decode(&result); err != nil {
		return nil, fmt.Errorf("decode Modrinth search: %w", err)
	}
	projects := make([]protocol.Project, 0, len(result.Hits))
	for _, hit := range result.Hits {
		if hit.Slug == "" || hit.Title == "" {
			continue
		}
		projects = append(projects, protocol.Project{
			Slug: hit.Slug, Title: hit.Title, Description: hit.Description,
			Author: hit.Author, Downloads: hit.Downloads, IconURL: hit.IconURL,
			ProjectType: hit.ProjectType, Categories: hit.Categories,
		})
	}
	return projects, nil
}

func Install(ctx context.Context, client *http.Client, slug, gameVersion, loader, category, gameDirectory string, writer *protocol.Writer) (string, error) {
	return installAt(ctx, client, apiURL, slug, gameVersion, loader, category, gameDirectory, writer)
}

func installAt(ctx context.Context, client *http.Client, baseURL, slug, gameVersion, loader, category, gameDirectory string, writer *protocol.Writer) (string, error) {
	if client == nil {
		client = &http.Client{Timeout: 10 * time.Minute}
	}
	kind := projectType(category)
	if kind == "mod" && strings.EqualFold(loader, "vanilla") {
		return "", fmt.Errorf("выбери Fabric, Forge, NeoForge или Quilt перед установкой модов")
	}
	if kind == "modpack" {
		return "", fmt.Errorf("автоматическая установка .mrpack будет доступна после реализации разрешения зависимостей")
	}
	if slug == "" || strings.ContainsAny(slug, `/\`) || strings.Contains(slug, "..") {
		return "", fmt.Errorf("invalid Modrinth project slug")
	}
	endpoint := strings.TrimRight(baseURL, "/") + "/project/" + url.PathEscape(slug) + "/version"
	parameters := url.Values{}
	if gameVersion != "" && gameVersion != "latest-release" {
		versions, _ := json.Marshal([]string{gameVersion})
		parameters.Set("game_versions", string(versions))
	}
	if kind == "mod" {
		loaders, _ := json.Marshal([]string{strings.ToLower(loader)})
		parameters.Set("loaders", string(loaders))
	}
	if len(parameters) > 0 {
		endpoint += "?" + parameters.Encode()
	}
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, endpoint, nil)
	if err != nil {
		return "", fmt.Errorf("create Modrinth versions request: %w", err)
	}
	request.Header.Set("User-Agent", "MintyLauncher/2.0.0-global")
	response, err := client.Do(request)
	if err != nil {
		return "", fmt.Errorf("fetch Modrinth versions: %w", err)
	}
	defer response.Body.Close()
	if response.StatusCode < 200 || response.StatusCode >= 300 {
		return "", fmt.Errorf("Modrinth returned HTTP %s for project versions", response.Status)
	}
	var versions []projectVersion
	if err := json.NewDecoder(response.Body).Decode(&versions); err != nil {
		return "", fmt.Errorf("decode Modrinth versions: %w", err)
	}
	if len(versions) == 0 {
		return "", fmt.Errorf("для проекта нет версии, совместимой с выбранной игрой и загрузчиком")
	}
	var selected *projectVersion
	for index := range versions {
		if gameVersion == "" || gameVersion == "latest-release" || contains(versions[index].GameVersions, gameVersion) {
			selected = &versions[index]
			break
		}
	}
	if selected == nil {
		return "", fmt.Errorf("проект не совместим с Minecraft %s", gameVersion)
	}
	if len(selected.Files) == 0 {
		return "", fmt.Errorf("Modrinth version has no downloadable files")
	}
	file := selected.Files[0]
	for _, candidate := range selected.Files {
		if candidate.Primary {
			file = candidate
			break
		}
	}
	if !strings.HasPrefix(strings.ToLower(file.URL), "https://") {
		return "", fmt.Errorf("Modrinth supplied a non-HTTPS file URL")
	}
	filename := filepath.Base(filepath.FromSlash(file.Filename))
	if filename == "." || filename == string(filepath.Separator) || filename == "" {
		return "", fmt.Errorf("Modrinth supplied an invalid filename")
	}
	if kind == "resourcepack" || kind == "shader" {
		if !strings.HasSuffix(strings.ToLower(filename), ".zip") {
			return "", fmt.Errorf("resource packs and shaders must be ZIP files")
		}
	}
	folder := "mods"
	if kind == "resourcepack" {
		folder = "resourcepacks"
	} else if kind == "shader" {
		folder = "shaderpacks"
	}
	if kind == "mod" && !strings.HasSuffix(strings.ToLower(filename), ".jar") {
		return "", fmt.Errorf("Modrinth mod download is not a JAR")
	}
	destinationDirectory := filepath.Join(gameDirectory, folder)
	destination := filepath.Join(destinationDirectory, filename)
	if filepath.Dir(destination) != destinationDirectory {
		return "", fmt.Errorf("invalid Modrinth download path")
	}
	if file.Size <= 0 {
		return "", fmt.Errorf("Modrinth did not provide the download size")
	}
	checksum := file.Hashes["sha1"]
	if len(checksum) != 40 {
		return "", fmt.Errorf("Modrinth did not provide a valid SHA-1 checksum")
	}
	if err := download.Fetch(ctx, client, file.URL, destination, checksum, file.Size, func(progress download.Progress) {
		fraction := float64(progress.Downloaded) / float64(progress.Total)
		_ = writer.Send(protocol.Event{Type: "download_progress", Message: filename, Progress: fraction,
			Downloaded: progress.Downloaded, Total: progress.Total, BytesPerSecond: progress.BytesPerSecond, ETASeconds: progress.ETASeconds})
	}); err != nil {
		return "", fmt.Errorf("install Modrinth project: %w", err)
	}
	return destination, nil
}

func projectType(category string) string {
	switch strings.ToLower(category) {
	case "modpack", "world":
		return "modpack"
	case "resourcepack":
		return "resourcepack"
	case "shader":
		return "shader"
	default:
		return "mod"
	}
}

func loaderFacet(loader string) string {
	switch strings.ToLower(loader) {
	case "fabric":
		return "fabric"
	case "forge":
		return "forge"
	case "neoforge":
		return "neoforge"
	case "quilt":
		return "quilt"
	default:
		return ""
	}
}

func contains(items []string, value string) bool {
	for _, item := range items {
		if item == value {
			return true
		}
	}
	return false
}
