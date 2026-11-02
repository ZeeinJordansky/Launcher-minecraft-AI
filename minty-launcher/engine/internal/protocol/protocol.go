package protocol

import (
	"encoding/json"
	"io"
	"sync"

	"minty-engine/internal/settings"
)

type Request struct {
	Action   string `json:"action"`
	GameDir  string `json:"gameDir,omitempty"`
	Version  string `json:"version,omitempty"`
	Username string `json:"username,omitempty"`
	JavaPath string `json:"javaPath,omitempty"`
	MemoryMB int    `json:"memoryMb,omitempty"`
	Settings *settings.Settings `json:"settings,omitempty"`
	Query    string `json:"query,omitempty"`
	Category string `json:"category,omitempty"`
	Loader   string `json:"loader,omitempty"`
	Sort     string `json:"sort,omitempty"`
	Slug     string `json:"slug,omitempty"`
}

type Version struct {
	ID          string `json:"id"`
	Type        string `json:"type"`
	ReleaseTime string `json:"releaseTime"`
}

type Event struct {
	Type           string    `json:"type"`
	Message        string    `json:"message,omitempty"`
	Version        string    `json:"version,omitempty"`
	Versions       []Version `json:"versions,omitempty"`
	Progress       float64   `json:"progress,omitempty"`
	Downloaded     int64     `json:"downloadedBytes,omitempty"`
	Total          int64     `json:"totalBytes,omitempty"`
	BytesPerSecond float64   `json:"bytesPerSecond,omitempty"`
	ETASeconds     int64     `json:"etaSeconds,omitempty"`
	Settings       *settings.Settings `json:"settings,omitempty"`
	Projects       []Project `json:"projects,omitempty"`
}

type Project struct {
	Slug        string   `json:"slug"`
	Title       string   `json:"title"`
	Description string   `json:"description"`
	Author      string   `json:"author"`
	Downloads   int64    `json:"downloads"`
	IconURL     string   `json:"iconUrl"`
	ProjectType string   `json:"projectType"`
	Categories  []string `json:"categories"`
}

type Writer struct {
	encoder *json.Encoder
	mutex   sync.Mutex
}

func NewWriter(destination io.Writer) *Writer {
	return &Writer{encoder: json.NewEncoder(destination)}
}

func (writer *Writer) Send(event Event) error {
	writer.mutex.Lock()
	defer writer.mutex.Unlock()
	return writer.encoder.Encode(event)
}
