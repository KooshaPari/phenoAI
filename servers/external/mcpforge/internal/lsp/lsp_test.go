package lsp

import (
	"encoding/json"
	"testing"

	"github.com/KooshaPari/MCPForge/internal/protocol"
)

func TestDetectLanguageID(t *testing.T) {
	tests := []struct {
		name     string
		uri      string
		expected protocol.LanguageKind
	}{
		// Go
		{"Go file", "main.go", protocol.LangGo},
		{"Go file with path", "/src/pkg/main.go", protocol.LangGo},

		// TypeScript / JavaScript
		{"TypeScript file", "app.ts", protocol.LangTypeScript},
		{"TypeScript React file", "component.tsx", protocol.LangTypeScriptReact},
		{"JavaScript file", "index.js", protocol.LangJavaScript},
		{"JavaScript React file", "app.jsx", protocol.LangJavaScriptReact},

		// Python
		{"Python file", "script.py", protocol.LangPython},

		// C/C++
		{"C file", "main.c", protocol.LangC},
		{"C++ file", "main.cpp", protocol.LangCPP},
		{"C++ cc file", "main.cc", protocol.LangCPP},
		{"C++ cxx file", "main.cxx", protocol.LangCPP},
		{"C++ c++ file", "main.c++", protocol.LangCPP},

		// Rust
		{"Rust file", "lib.rs", protocol.LangRust},

		// Java
		{"Java file", "Main.java", protocol.LangJava},

		// Web languages
		{"HTML file", "index.html", protocol.LangHTML},
		{"HTML htm file", "index.htm", protocol.LangHTML},
		{"CSS file", "style.css", protocol.LangCSS},
		{"SCSS file", "style.scss", protocol.LangSCSS},
		{"JSON file", "config.json", protocol.LangJSON},

		// Shell
		{"Bash file", "script.sh", protocol.LangShellScript},
		{"Zsh file", "script.zsh", protocol.LangShellScript},
		{"Shell file (no extension)", "script", protocol.LanguageKind("")},

		// Ruby
		{"Ruby file", "script.rb", protocol.LangRuby},

		// PHP
		{"PHP file", "script.php", protocol.LangPHP},

		// Swift
		{"Swift file", "script.swift", protocol.LangSwift},

		// YAML
		{"YAML file", "config.yaml", protocol.LangYAML},
		{"YML file", "config.yml", protocol.LangYAML},

		// Markdown
		{"Markdown file", "README.md", protocol.LangMarkdown},
		{"Markdown alternate", "README.markdown", protocol.LangMarkdown},

		// Makefile - note: implementation checks ".makefile" but not "makefile"
		{"Makefile capitalized", "Makefile", protocol.LanguageKind("")},
		{"Makefile lowercase (no dot)", "makefile", protocol.LanguageKind("")},
		{"Makefile with dot", ".makefile", protocol.LangMakefile},

		// Unknown
		{"Unknown extension", "file.xyz", protocol.LanguageKind("")},
		{"No extension", "file", protocol.LanguageKind("")},
		{"Empty string", "", protocol.LanguageKind("")},

		// Case insensitivity
		{"Uppercase extension", "file.TS", protocol.LangTypeScript},
		{"Mixed case", "file.Py", protocol.LangPython},

		// Edge cases
		// Note: Dockerfile detection requires ".dockerfile" extension, not bare filename
		{"Dockerfile no extension", "Dockerfile", protocol.LanguageKind("")},
		{"Gitignore file", ".gitignore", protocol.LanguageKind("")},
		// Note: Shell detection only works with specific extensions, not arbitrary hidden files
		// .bashrc has extension .bashrc which doesn't match .sh, .bash, .zsh, .ksh
		{"Hidden file with shell extension", ".bash", protocol.LangShellScript},
		{"Hidden file with .zsh extension", ".zshrc", protocol.LanguageKind("")},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := DetectLanguageID(tt.uri)
			if result != tt.expected {
				t.Errorf("DetectLanguageID(%q) = %v, want %v", tt.uri, result, tt.expected)
			}
		})
	}
}

func TestMessageID_MarshalJSON(t *testing.T) {
	tests := []struct {
		name     string
		id       *MessageID
		expected string
	}{
		{
			name:     "Nil ID",
			id:       nil,
			expected: "null",
		},
		{
			name:     "Nil Value",
			id:       &MessageID{Value: nil},
			expected: "null",
		},
		{
			name:     "Integer ID",
			id:       &MessageID{Value: int32(42)},
			expected: "42",
		},
		{
			name:     "String ID",
			id:       &MessageID{Value: "request-1"},
			expected: `"request-1"`,
		},
		{
			name:     "Float ID",
			id:       &MessageID{Value: float64(3.14)},
			expected: "3.14",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result, err := tt.id.MarshalJSON()
			if err != nil {
				t.Errorf("MarshalJSON() error = %v", err)
				return
			}
			if string(result) != tt.expected {
				t.Errorf("MarshalJSON() = %v, want %v", string(result), tt.expected)
			}
		})
	}
}

func TestMessageID_UnmarshalJSON(t *testing.T) {
	tests := []struct {
		name     string
		data     string
		expected interface{}
	}{
		{
			name:     "Null value",
			data:     "null",
			expected: nil,
		},
		{
			name:     "Integer value",
			data:     "42",
			expected: int32(42),
		},
		{
			name:     "String value",
			data:     `"request-1"`,
			expected: "request-1",
		},
		{
			name:     "Float value - truncated to int32",
			data:     "3.14",
			expected: int32(3), // Implementation truncates floats to int32
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var id MessageID
			err := json.Unmarshal([]byte(tt.data), &id)
			if err != nil {
				t.Errorf("UnmarshalJSON() error = %v", err)
				return
			}
			if tt.expected == nil {
				if id.Value != nil {
					t.Errorf("UnmarshalJSON() = %v, want nil", id.Value)
				}
			} else if id.Value != tt.expected {
				t.Errorf("UnmarshalJSON() = %v, want %v", id.Value, tt.expected)
			}
		})
	}
}

func TestMessageID_String(t *testing.T) {
	tests := []struct {
		name     string
		id       *MessageID
		expected string
	}{
		{
			name:     "Nil ID",
			id:       nil,
			expected: "<null>",
		},
		{
			name:     "Nil Value",
			id:       &MessageID{Value: nil},
			expected: "<null>",
		},
		{
			name:     "Integer ID",
			id:       &MessageID{Value: int32(42)},
			expected: "42",
		},
		{
			name:     "String ID",
			id:       &MessageID{Value: "request-1"},
			expected: "request-1",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := tt.id.String()
			if result != tt.expected {
				t.Errorf("String() = %v, want %v", result, tt.expected)
			}
		})
	}
}

func TestMessageID_Equals(t *testing.T) {
	tests := []struct {
		name     string
		id1      *MessageID
		id2      *MessageID
		expected bool
	}{
		{
			name:     "Both nil",
			id1:      nil,
			id2:      nil,
			expected: true,
		},
		{
			name:     "First nil",
			id1:      nil,
			id2:      &MessageID{Value: int32(1)},
			expected: false,
		},
		{
			name:     "Second nil",
			id1:      &MessageID{Value: int32(1)},
			id2:      nil,
			expected: false,
		},
		{
			name:     "Both nil values",
			id1:      &MessageID{Value: nil},
			id2:      &MessageID{Value: nil},
			expected: true,
		},
		{
			name:     "Same integer values",
			id1:      &MessageID{Value: int32(42)},
			id2:      &MessageID{Value: int32(42)},
			expected: true,
		},
		{
			name:     "Different integer values",
			id1:      &MessageID{Value: int32(42)},
			id2:      &MessageID{Value: int32(100)},
			expected: false,
		},
		{
			name:     "Same string values",
			id1:      &MessageID{Value: "request-1"},
			id2:      &MessageID{Value: "request-1"},
			expected: true,
		},
		{
			name:     "Different string values",
			id1:      &MessageID{Value: "request-1"},
			id2:      &MessageID{Value: "request-2"},
			expected: false,
		},
		{
			name:     "Int and string same value",
			id1:      &MessageID{Value: int32(42)},
			id2:      &MessageID{Value: "42"},
			expected: true, // String representation comparison
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := tt.id1.Equals(tt.id2)
			if result != tt.expected {
				t.Errorf("Equals() = %v, want %v", result, tt.expected)
			}
		})
	}
}

func TestNewRequest(t *testing.T) {
	tests := []struct {
		name        string
		id          any
		method      string
		params      any
		expectError bool
		checkResult func(*testing.T, *Message)
	}{
		{
			name:        "Valid request with integer ID",
			id:          int32(1),
			method:      "textDocument/definition",
			params:      map[string]string{"textDocument": "file:///test.go"},
			expectError: false,
			checkResult: func(t *testing.T, msg *Message) {
				if msg.JSONRPC != "2.0" {
					t.Errorf("JSONRPC = %v, want 2.0", msg.JSONRPC)
				}
				if msg.ID == nil || msg.ID.Value != int32(1) {
					t.Errorf("ID = %v, want 1", msg.ID)
				}
				if msg.Method != "textDocument/definition" {
					t.Errorf("Method = %v, want textDocument/definition", msg.Method)
				}
				if msg.Params == nil {
					t.Errorf("Params = nil, want non-nil")
				}
			},
		},
		{
			name:        "Valid request with string ID",
			id:          "request-1",
			method:      "initialize",
			params:      nil,
			expectError: false,
			checkResult: func(t *testing.T, msg *Message) {
				if msg.ID == nil || msg.ID.Value != "request-1" {
					t.Errorf("ID = %v, want request-1", msg.ID)
				}
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result, err := NewRequest(tt.id, tt.method, tt.params)
			if tt.expectError {
				if err == nil {
					t.Errorf("Expected error but got none")
				}
				return
			}
			if err != nil {
				t.Errorf("Unexpected error: %v", err)
				return
			}
			tt.checkResult(t, result)
		})
	}
}

func TestNewNotification(t *testing.T) {
	tests := []struct {
		name        string
		method      string
		params      any
		expectError bool
		checkResult func(*testing.T, *Message)
	}{
		{
			name:        "Valid notification",
			method:      "textDocument/didOpen",
			params:      map[string]any{"textDocument": map[string]any{"uri": "file:///test.go"}},
			expectError: false,
			checkResult: func(t *testing.T, msg *Message) {
				if msg.JSONRPC != "2.0" {
					t.Errorf("JSONRPC = %v, want 2.0", msg.JSONRPC)
				}
				if msg.ID != nil {
					t.Errorf("ID = %v, want nil for notification", msg.ID)
				}
				if msg.Method != "textDocument/didOpen" {
					t.Errorf("Method = %v, want textDocument/didOpen", msg.Method)
				}
				if msg.Params == nil {
					t.Errorf("Params = nil, want non-nil")
				}
			},
		},
		{
			name:        "Notification without params",
			method:      "initialized",
			params:      nil,
			expectError: false,
			checkResult: func(t *testing.T, msg *Message) {
				if msg.Method != "initialized" {
					t.Errorf("Method = %v, want initialized", msg.Method)
				}
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result, err := NewNotification(tt.method, tt.params)
			if tt.expectError {
				if err == nil {
					t.Errorf("Expected error but got none")
				}
				return
			}
			if err != nil {
				t.Errorf("Unexpected error: %v", err)
				return
			}
			tt.checkResult(t, result)
		})
	}
}

func TestMessage_FullCycle(t *testing.T) {
	// Test that a message can be marshaled and unmarshaled correctly
	original := &Message{
		JSONRPC: "2.0",
		ID:      &MessageID{Value: int32(42)},
		Method:  "test/method",
		Params:  json.RawMessage(`{"key":"value"}`),
	}

	// Marshal
	data, err := json.Marshal(original)
	if err != nil {
		t.Fatalf("Marshal error: %v", err)
	}

	// Unmarshal
	var decoded Message
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("Unmarshal error: %v", err)
	}

	// Verify
	if decoded.JSONRPC != original.JSONRPC {
		t.Errorf("JSONRPC = %v, want %v", decoded.JSONRPC, original.JSONRPC)
	}
	if !decoded.ID.Equals(original.ID) {
		t.Errorf("ID = %v, want %v", decoded.ID, original.ID)
	}
	if decoded.Method != original.Method {
		t.Errorf("Method = %v, want %v", decoded.Method, original.Method)
	}
	if string(decoded.Params) != string(original.Params) {
		t.Errorf("Params = %v, want %v", string(decoded.Params), string(original.Params))
	}
}
