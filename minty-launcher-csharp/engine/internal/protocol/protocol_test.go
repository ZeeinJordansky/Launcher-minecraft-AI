package protocol

import (
	"bytes"
	"encoding/json"
	"testing"
)

func TestWriterEmitsOneJSONEventPerLine(t *testing.T) {
	var output bytes.Buffer
	writer := NewWriter(&output)
	if err := writer.Send(Event{Type: "download_progress", Progress: 0.5, BytesPerSecond: 1024, ETASeconds: 4}); err != nil {
		t.Fatalf("Send() error = %v", err)
	}

	var event Event
	if err := json.Unmarshal(bytes.TrimSpace(output.Bytes()), &event); err != nil {
		t.Fatalf("event is not valid JSON: %v", err)
	}
	if event.Type != "download_progress" || event.Progress != 0.5 || event.ETASeconds != 4 {
		t.Fatalf("unexpected event payload: %+v", event)
	}
}
