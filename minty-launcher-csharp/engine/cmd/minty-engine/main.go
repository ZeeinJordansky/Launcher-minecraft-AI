package main

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"os/signal"
	"time"

	"minty-engine/internal/minecraft"
	"minty-engine/internal/mods"
	"minty-engine/internal/protocol"
	"minty-engine/internal/settings"
)

func main() {
	writer := protocol.NewWriter(os.Stdout)
	var request protocol.Request
	if err := json.NewDecoder(os.Stdin).Decode(&request); err != nil {
		fail(writer, fmt.Errorf("read engine request: %w", err))
		os.Exit(1)
	}

	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt)
	defer stop()
	var err error
	switch request.Action {
	case "list_versions":
		client := &http.Client{Timeout: 45 * time.Second}
		versions, latest, listErr := minecraft.ListVersions(ctx, client)
		if listErr != nil {
			err = listErr
			break
		}
		err = writer.Send(protocol.Event{Type: "versions", Version: latest, Versions: versions})
	case "get_settings":
		var value settings.Settings
		value, err = settings.Load()
		if err == nil {
			err = writer.Send(protocol.Event{Type: "settings", Settings: &value})
		}
	case "save_settings":
		if request.Settings == nil {
			err = fmt.Errorf("settings are required")
			break
		}
		err = settings.Save(*request.Settings)
		if err == nil {
			err = writer.Send(protocol.Event{Type: "complete", Message: "Настройки сохранены."})
		}
	case "install_version":
		if request.Settings == nil {
			err = fmt.Errorf("settings are required")
			break
		}
		err = minecraft.InstallVersion(ctx, *request.Settings, writer)
	case "launch":
		if request.Settings == nil {
			err = fmt.Errorf("settings are required")
			break
		}
		err = minecraft.Launch(ctx, *request.Settings, writer)
	case "search_mods":
		configuration := settings.Defaults()
		if request.Settings != nil {
			configuration = *request.Settings
		}
		version := request.Version
		if version == "" {
			version = configuration.SelectedVersion
		}
		loader := request.Loader
		if loader == "" {
			loader = configuration.ModLoader
		}
		projects, searchErr := mods.Search(ctx, nil, request.Query, version, loader, request.Category, request.Sort)
		if searchErr != nil {
			err = searchErr
			break
		}
		err = writer.Send(protocol.Event{Type: "projects", Projects: projects})
	case "install_mod":
		if request.Settings == nil {
			err = fmt.Errorf("settings are required")
			break
		}
		version := request.Version
		if version == "" {
			version = request.Settings.SelectedVersion
		}
		configuration := *request.Settings
		configuration.SelectedVersion = version
		gameDirectory, directoryErr := settings.ProfileDirectory(configuration)
		if directoryErr != nil {
			err = directoryErr
			break
		}
		loader := request.Loader
		if loader == "" {
			loader = request.Settings.ModLoader
		}
		path, installErr := mods.Install(ctx, nil, request.Slug, version, loader, request.Category, gameDirectory, writer)
		if installErr != nil {
			err = installErr
			break
		}
		err = writer.Send(protocol.Event{Type: "complete", Message: "Установлено: " + path})
	default:
		err = fmt.Errorf("unknown engine action %q", request.Action)
	}
	if err != nil {
		fail(writer, err)
		os.Exit(1)
	}
}

func fail(writer *protocol.Writer, err error) {
	_ = writer.Send(protocol.Event{Type: "error", Message: err.Error()})
}
