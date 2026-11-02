package download

import (
	"context"
	"crypto/sha1"
	"encoding/hex"
	"errors"
	"fmt"
	"hash"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"time"
)

type Progress struct {
	Downloaded     int64
	Total          int64
	BytesPerSecond float64
	ETASeconds     int64
}

type Reporter func(Progress)

func Fetch(ctx context.Context, client *http.Client, url, destination, expectedSHA1 string, expectedSize int64, report Reporter) error {
	if client == nil {
		client = &http.Client{Timeout: 10 * time.Minute}
	}
	if expectedSize > 0 && fileMatches(destination, expectedSHA1, expectedSize) {
		if report != nil {
			report(Progress{Downloaded: expectedSize, Total: expectedSize})
		}
		return nil
	}

	request, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return fmt.Errorf("create download request: %w", err)
	}
	request.Header.Set("User-Agent", "MintyLauncher/2.0.0-global")
	response, err := client.Do(request)
	if err != nil {
		return fmt.Errorf("download %s: %w", url, err)
	}
	defer response.Body.Close()
	if response.StatusCode < 200 || response.StatusCode >= 300 {
		return fmt.Errorf("download %s: HTTP %s", url, response.Status)
	}

	total := expectedSize
	if total <= 0 {
		total = response.ContentLength
	}
	if err := os.MkdirAll(filepath.Dir(destination), 0o755); err != nil {
		return fmt.Errorf("create download directory: %w", err)
	}
	temporary, err := os.CreateTemp(filepath.Dir(destination), ".minty-*.partial")
	if err != nil {
		return fmt.Errorf("create temporary download: %w", err)
	}
	temporaryPath := temporary.Name()
	defer os.Remove(temporaryPath)

	started := time.Now()
	tracker := &speedTracker{total: total, started: started, lastAt: started}
	checksum := sha1.New()
	progress := &progressWriter{destination: io.MultiWriter(temporary, checksum), hash: checksum, tracker: tracker, report: report}
	if _, err := io.CopyBuffer(progress, response.Body, make([]byte, 128*1024)); err != nil {
		_ = temporary.Close()
		return fmt.Errorf("write download: %w", err)
	}
	if err := temporary.Sync(); err != nil {
		_ = temporary.Close()
		return fmt.Errorf("flush download: %w", err)
	}
	if err := temporary.Close(); err != nil {
		return fmt.Errorf("close download: %w", err)
	}

	if total > 0 && tracker.downloaded != total {
		return fmt.Errorf("download size mismatch: received %d bytes, expected %d", tracker.downloaded, total)
	}
	if expectedSHA1 != "" {
		actual := hex.EncodeToString(checksum.Sum(nil))
		if !strings.EqualFold(actual, expectedSHA1) {
			return fmt.Errorf("SHA-1 mismatch for %s", url)
		}
	}
	if err := os.Remove(destination); err != nil && !errors.Is(err, os.ErrNotExist) {
		return fmt.Errorf("replace cached file: %w", err)
	}
	if err := os.Rename(temporaryPath, destination); err != nil {
		return fmt.Errorf("finalize download: %w", err)
	}
	if report != nil {
		report(tracker.snapshot(true))
	}
	return nil
}

func fileMatches(path, expectedSHA1 string, expectedSize int64) bool {
	info, err := os.Stat(path)
	if err != nil || !info.Mode().IsRegular() || info.Size() != expectedSize {
		return false
	}
	if expectedSHA1 == "" {
		return true
	}
	file, err := os.Open(path)
	if err != nil {
		return false
	}
	defer file.Close()
	hash := sha1.New()
	if _, err := io.Copy(hash, file); err != nil {
		return false
	}
	return strings.EqualFold(hex.EncodeToString(hash.Sum(nil)), expectedSHA1)
}

type progressWriter struct {
	destination io.Writer
	hash        hash.Hash
	tracker     *speedTracker
	report      Reporter
}

func (writer *progressWriter) Write(data []byte) (int, error) {
	written, err := writer.destination.Write(data)
	if written > 0 {
		writer.tracker.downloaded += int64(written)
		if writer.report != nil {
			writer.report(writer.tracker.snapshot(false))
		}
	}
	return written, err
}

type speedTracker struct {
	total      int64
	downloaded int64
	started    time.Time
	lastAt     time.Time
	lastBytes  int64
}

func (tracker *speedTracker) snapshot(final bool) Progress {
	now := time.Now()
	elapsed := now.Sub(tracker.started).Seconds()
	if elapsed <= 0 {
		elapsed = 0.001
	}
	speed := float64(tracker.downloaded) / elapsed
	if !final && now.Sub(tracker.lastAt) >= 300*time.Millisecond {
		window := now.Sub(tracker.lastAt).Seconds()
		if window > 0 {
			speed = float64(tracker.downloaded-tracker.lastBytes) / window
		}
		tracker.lastAt = now
		tracker.lastBytes = tracker.downloaded
	}
	eta := int64(-1)
	if tracker.total > 0 && speed > 0 {
		remaining := tracker.total - tracker.downloaded
		if remaining < 0 {
			remaining = 0
		}
		eta = int64(float64(remaining) / speed)
	}
	return Progress{Downloaded: tracker.downloaded, Total: tracker.total, BytesPerSecond: speed, ETASeconds: eta}
}
