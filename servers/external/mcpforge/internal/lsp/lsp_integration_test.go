package lsp

import (
	"bufio"
	"bytes"
	"encoding/json"
	"sync"
	"testing"

	"github.com/KooshaPari/MCPForge/internal/protocol"
)

// TestLSPMessageFlow_FullRequestResponseCycle exercises the complete
// server-to-client request/response cycle that the LSP client uses internally
// to handle requests from the language server. It does not spawn an actual LSP
// server process; instead it drives the same transport primitives
// (NewRequest, WriteMessage, ReadMessage) and the same handler dispatch logic
// (see handleMessages in transport.go) that the Client uses when an LSP server
// asks the client for configuration or registers a capability.
//
// The flow verified for each method is:
//
//  1. Build a JSON-RPC request Message from typed Go params (NewRequest).
//  2. Write it to an in-memory wire (WriteMessage) — produces the same
//     `Content-Length` framed bytes the client writes to the server.
//  3. Read it back from the wire (ReadMessage) — same parser the client
//     uses on its stdout.
//  4. Verify the unmarshaled Message preserves method, id, and params.
//  5. Dispatch to the registered handler, capturing the result.
//  6. Marshal the result into a response Message, write it back through
//     the transport, and read it to confirm the round-trip is lossless.
//
// The test covers both a request that returns a real result payload
// (workspace/configuration) and a request that has a rich side-effect
// (client/registerCapability with a workspace/didChangeWatchedFiles
// registration, which must dispatch to the file watch handler).
func TestLSPMessageFlow_FullRequestResponseCycle(t *testing.T) {
	// --- Phase 1: workspace/configuration request/response -------------------
	//
	// The server asks the client for configuration. HandleWorkspaceConfiguration
	// returns []map[string]any{{}} per the LSP spec. We verify the result
	// survives a wire round-trip exactly as a real LSP server would receive it.
	t.Run("workspace_configuration_round_trip", func(t *testing.T) {
		// (1) Build the request the server would send us.
		req, err := NewRequest(int32(1), "workspace/configuration", map[string]any{
			"items": []map[string]any{{"section": "test"}},
		})
		if err != nil {
			t.Fatalf("NewRequest failed: %v", err)
		}

		// (2) Write it to a wire buffer — this is what the client sees on stdout.
		var wire bytes.Buffer
		if err := WriteMessage(&wire, req); err != nil {
			t.Fatalf("WriteMessage failed: %v", err)
		}

		// (3) Read it back — this is what handleMessages does first.
		got, err := ReadMessage(bufio.NewReader(&wire))
		if err != nil {
			t.Fatalf("ReadMessage failed: %v", err)
		}

		// (4) Verify wire-level integrity: method, id, params.
		if got.JSONRPC != "2.0" {
			t.Errorf("JSONRPC = %q, want 2.0", got.JSONRPC)
		}
		if got.Method != "workspace/configuration" {
			t.Errorf("Method = %q, want workspace/configuration", got.Method)
		}
		if got.ID == nil || got.ID.Value != int32(1) {
			t.Errorf("ID = %v, want 1", got.ID)
		}
		// Params must still contain the items array we sent.
		var params struct {
			Items []map[string]any `json:"items"`
		}
		if err := json.Unmarshal(got.Params, &params); err != nil {
			t.Fatalf("unmarshal params: %v", err)
		}
		if len(params.Items) != 1 || params.Items[0]["section"] != "test" {
			t.Errorf("params round-trip mismatch: %+v", params)
		}

		// (5) Dispatch to the real handler.
		result, err := HandleWorkspaceConfiguration(got.Params)
		if err != nil {
			t.Fatalf("HandleWorkspaceConfiguration returned error: %v", err)
		}

		// (6) Build a response Message and send it back through the wire.
		resultJSON, err := json.Marshal(result)
		if err != nil {
			t.Fatalf("marshal result: %v", err)
		}
		resp := &Message{
			JSONRPC: "2.0",
			ID:      got.ID,
			Result:  resultJSON,
		}
		var respWire bytes.Buffer
		if err := WriteMessage(&respWire, resp); err != nil {
			t.Fatalf("WriteMessage(response) failed: %v", err)
		}
		respGot, err := ReadMessage(bufio.NewReader(&respWire))
		if err != nil {
			t.Fatalf("ReadMessage(response) failed: %v", err)
		}

		// Response must echo the id, carry a result, and have no error.
		if respGot.ID == nil || !respGot.ID.Equals(got.ID) {
			t.Errorf("response ID = %v, want %v", respGot.ID, got.ID)
		}
		if respGot.Error != nil {
			t.Errorf("response Error = %+v, want nil", respGot.Error)
		}
		// The result must be a single-element array of empty config objects.
		var got2 []map[string]any
		if err := json.Unmarshal(respGot.Result, &got2); err != nil {
			t.Fatalf("unmarshal response result: %v", err)
		}
		if len(got2) != 1 {
			t.Fatalf("response result length = %d, want 1", len(got2))
		}
		if len(got2[0]) != 0 {
			t.Errorf("response result[0] = %+v, want empty object", got2[0])
		}
	})

	// --- Phase 2: client/registerCapability with file watcher ---------------
	//
	// The server asks the client to register a workspace/didChangeWatchedFiles
	// capability. HandleRegisterCapability must (a) parse the nested
	// registration options, (b) dispatch the resulting FileSystemWatcher list
	// to whatever FileWatchHandler is currently registered, and (c) return a
	// nil result on the JSON-RPC response.
	t.Run("register_capability_dispatches_file_watcher", func(t *testing.T) {
		// Register a file watch handler that captures the call. Restore the
		// previous handler when the test finishes so we don't leak global
		// state into other tests in this package.
		prev := fileWatchHandler
		t.Cleanup(func() { fileWatchHandler = prev })

		var (
			captureMu sync.Mutex
			captured  struct {
				id       string
				watchers []protocol.FileSystemWatcher
			}
		)
		RegisterFileWatchHandler(func(id string, watchers []protocol.FileSystemWatcher) {
			captureMu.Lock()
			defer captureMu.Unlock()
			captured.id = id
			captured.watchers = watchers
		})

		// (1) Build the server->client request the real server would emit.
		reqParams := protocol.RegistrationParams{
			Registrations: []protocol.Registration{
				{
					ID:     "watcher-1",
					Method: "workspace/didChangeWatchedFiles",
					RegisterOptions: protocol.DidChangeWatchedFilesRegistrationOptions{
						Watchers: []protocol.FileSystemWatcher{
							{
								GlobPattern: protocol.Or_GlobPattern{Value: "**/*.go"},
								Kind:        kindPtr(protocol.WatchCreate | protocol.WatchChange | protocol.WatchDelete),
							},
							{
								GlobPattern: protocol.Or_GlobPattern{Value: "**/*.ts"},
							},
						},
					},
				},
				{
					// Non-file-watcher registration; handler should still accept
					// it but must not invoke the file watch callback.
					ID:     "other-cap-1",
					Method: "workspace/semanticTokens/refresh",
				},
			},
		}
		req, err := NewRequest("reg-1", "client/registerCapability", reqParams)
		if err != nil {
			t.Fatalf("NewRequest failed: %v", err)
		}

		// (2) Round-trip the request through the wire.
		var wire bytes.Buffer
		if err := WriteMessage(&wire, req); err != nil {
			t.Fatalf("WriteMessage failed: %v", err)
		}
		got, err := ReadMessage(bufio.NewReader(&wire))
		if err != nil {
			t.Fatalf("ReadMessage failed: %v", err)
		}
		if got.Method != "client/registerCapability" {
			t.Errorf("Method = %q, want client/registerCapability", got.Method)
		}
		if got.ID == nil || got.ID.Value != "reg-1" {
			t.Errorf("ID = %v, want reg-1", got.ID)
		}

		// (3) Dispatch to the real handler.
		result, err := HandleRegisterCapability(got.Params)
		if err != nil {
			t.Fatalf("HandleRegisterCapability returned error: %v", err)
		}
		if result != nil {
			t.Errorf("HandleRegisterCapability result = %v, want nil", result)
		}

		// (4) The file watch handler must have been called exactly once for
		// the file-watcher registration, with the parsed watcher list.
		captureMu.Lock()
		defer captureMu.Unlock()
		if captured.id != "watcher-1" {
			t.Errorf("file watch handler id = %q, want watcher-1", captured.id)
		}
		if len(captured.watchers) != 2 {
			t.Fatalf("watcher count = %d, want 2", len(captured.watchers))
		}
		// First watcher: explicit Kind, glob **/*.go
		if got := captured.watchers[0].GlobPattern.Value; got != "**/*.go" {
			t.Errorf("watcher[0].GlobPattern = %v, want **/*.go", got)
		}
		if captured.watchers[0].Kind == nil {
			t.Errorf("watcher[0].Kind = nil, want non-nil")
		} else if *captured.watchers[0].Kind !=
			(protocol.WatchCreate|protocol.WatchChange|protocol.WatchDelete) {
			t.Errorf("watcher[0].Kind = %d, want %d",
				*captured.watchers[0].Kind,
				protocol.WatchCreate|protocol.WatchChange|protocol.WatchDelete)
		}
		// Second watcher: nil Kind, glob **/*.ts
		if got := captured.watchers[1].GlobPattern.Value; got != "**/*.ts" {
			t.Errorf("watcher[1].GlobPattern = %v, want **/*.ts", got)
		}
		if captured.watchers[1].Kind != nil {
			t.Errorf("watcher[1].Kind = %v, want nil", captured.watchers[1].Kind)
		}

		// (5) Marshal the nil result into a response and round-trip it.
		resp := &Message{
			JSONRPC: "2.0",
			ID:      got.ID,
			Result:  json.RawMessage("null"),
		}
		var respWire bytes.Buffer
		if err := WriteMessage(&respWire, resp); err != nil {
			t.Fatalf("WriteMessage(response) failed: %v", err)
		}
		respGot, err := ReadMessage(bufio.NewReader(&respWire))
		if err != nil {
			t.Fatalf("ReadMessage(response) failed: %v", err)
		}
		if respGot.ID == nil || !respGot.ID.Equals(got.ID) {
			t.Errorf("response ID = %v, want %v", respGot.ID, got.ID)
		}
		if string(respGot.Result) != "null" {
			t.Errorf("response Result = %s, want null", string(respGot.Result))
		}
		if respGot.Error != nil {
			t.Errorf("response Error = %+v, want nil", respGot.Error)
		}
	})
}

// kindPtr is a small helper to take the address of a WatchKind literal.
func kindPtr(k protocol.WatchKind) *protocol.WatchKind { return &k }
