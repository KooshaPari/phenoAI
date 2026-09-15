package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"reflect"

	"github.com/invopop/jsonschema"
	mcp_golang "github.com/metoro-io/mcp-golang"
	"github.com/metoro-io/mcp-golang/transport/stdio"
)

// toolRegistration pairs a tool's metadata with its handler so we can both
// register it with the MCP server at runtime and emit a static manifest
// (tools.json) without hitting the SDK's private `tools` map.
//
// SDK limitation: github.com/metoro-io/mcp-golang@v0.13.0 stores registered
// tools in an unexported field on *Server and exposes no getter. Until
// upstream adds a ListTools()/Tools() accessor, we own the single source of
// truth here and reflect on argType to derive the JSON Schema the same way
// the SDK does (via invopop/jsonschema).
type toolRegistration struct {
	Name        string
	Description string
	ArgType     reflect.Type
	Handler     any
}

func toolRegistrations() []toolRegistration {
	return []toolRegistration{
		{"pkg_load", "Load package", reflect.TypeOf(MyFunctionsArguments{}), loadPackage},
		{"instance_logs", "Instance logs", reflect.TypeOf(InstanceArguments{}), instanceLogs},
		{"instance_create", "Instance create", reflect.TypeOf(InstanceArguments{}), instanceCreate},
		{"list_instances", "List instances", reflect.TypeOf(MyFunctionsArguments{}), listInstances},
		{"list_images", "List images", reflect.TypeOf(MyFunctionsArguments{}), listImages},
	}
}

// manifestTool matches the MCP tools/list shape: name, description, inputSchema.
type manifestTool struct {
	Name        string      `json:"name"`
	Description string      `json:"description"`
	InputSchema interface{} `json:"inputSchema"`
}

func dumpManifest(path string) error {
	reflector := &jsonschema.Reflector{
		DoNotReference:             true,
		AllowAdditionalProperties:  true,
		RequiredFromJSONSchemaTags: false,
		ExpandedStruct:             true,
	}

	regs := toolRegistrations()
	tools := make([]manifestTool, 0, len(regs))
	for _, r := range regs {
		schema := reflector.ReflectFromType(r.ArgType)
		tools = append(tools, manifestTool{
			Name:        r.Name,
			Description: r.Description,
			InputSchema: schema,
		})
	}

	out := map[string]interface{}{"tools": tools}
	f, err := os.Create(path)
	if err != nil {
		return err
	}
	defer f.Close()
	enc := json.NewEncoder(f)
	enc.SetIndent("", "  ")
	if err := enc.Encode(out); err != nil {
		return err
	}
	return nil
}

func main() {
	dumpTools := flag.Bool("dump-tools", false, "write tools.json manifest and exit")
	dumpPath := flag.String("dump-tools-path", "tools.json", "path for the emitted manifest")
	flag.Parse()

	if *dumpTools {
		if err := dumpManifest(*dumpPath); err != nil {
			fmt.Fprintf(os.Stderr, "dump-tools failed: %v\n", err)
			os.Exit(1)
		}
		return
	}

	done := make(chan struct{})

	server := mcp_golang.NewServer(stdio.NewStdioServerTransport())

	for _, r := range toolRegistrations() {
		if err := server.RegisterTool(r.Name, r.Description, r.Handler); err != nil {
			panic(err)
		}
	}

	if err := server.Serve(); err != nil {
		panic(err)
	}

	<-done
}
