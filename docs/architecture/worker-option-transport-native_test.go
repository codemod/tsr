package compiler

import (
	"encoding/json"
	"github.com/microsoft/typescript-go/internal/ast"
	"github.com/microsoft/typescript-go/internal/collections"
	"github.com/microsoft/typescript-go/internal/core"
	"github.com/microsoft/typescript-go/internal/tsoptions"
	"github.com/microsoft/typescript-go/internal/tspath"
	"github.com/microsoft/typescript-go/internal/vfs"
	"github.com/microsoft/typescript-go/internal/vfs/osvfs"
	"os"
	"testing"
)

type transportHost struct{ cwd string }

func (h transportHost) FS() vfs.FS                  { return osvfs.FS() }
func (h transportHost) GetCurrentDirectory() string { return h.cwd }
func TestTSRTransportNumbers(t *testing.T) {
	type errRow struct {
		Code int32    `json:"code"`
		Args []string `json:"args"`
	}
	type row struct {
		Input    string   `json:"input"`
		CLI      bool     `json:"cli"`
		Checkers *int     `json:"checkers"`
		Errors   []errRow `json:"errors"`
	}
	rows := []row{}
	host := transportHost{t.TempDir()}
	for _, input := range []string{"0", "-1", "1.5", "1", "2", "2147483648", "9007199254740993", "9223372036854775807", "9223372036854775808", "1e300", "null", "\"many\""} {
		for _, cli := range []bool{false, true} {
			var parsed *tsoptions.ParsedCommandLine
			if cli {
				arg := input
				if input == `"many"` {
					arg = "many"
				}
				parsed = tsoptions.ParseCommandLine([]string{"--checkers", arg}, host)
			} else {
				source, errors := tsoptions.ParseConfigFileTextToJson(host.cwd+"/tsconfig.json", tspath.Path(host.cwd+"/tsconfig.json"), `{"compilerOptions":{"checkers":`+input+`},"files":["main.ts"]}`)
				if len(errors) != 0 {
					t.Fatal(errors)
				}
				parsed = tsoptions.ParseJsonConfigFileContent(source, host, host.cwd, nil, "tsconfig.json", nil, nil, nil)
			}
			entries := []errRow{}
			for _, d := range parsed.Errors {
				entries = append(entries, errRow{d.Code(), d.MessageArgs()})
			}
			rows = append(rows, row{input, cli, parsed.CompilerOptions().Checkers, entries})
		}
	}
	var _ *ast.Diagnostic
	data, e := json.MarshalIndent(rows, "", "  ")
	if e != nil {
		t.Fatal(e)
	}
	if e = os.WriteFile(os.Getenv("TSR_TRANSPORT_ORACLE"), append(data, '\n'), 0600); e != nil {
		t.Fatal(e)
	}
}
func TestTSRTransportMerges(t *testing.T) {
	type row struct {
		Case     string  `json:"case"`
		Checkers *int    `json:"checkers"`
		Single   int     `json:"single"`
		Files    int     `json:"files"`
		Workers  int     `json:"workers"`
		Errors   []int32 `json:"errors"`
	}
	rows := []row{}
	directory := t.TempDir()
	host := transportHost{directory}
	write := func(name, text string) {
		if e := os.WriteFile(directory+"/"+name, []byte(text), 0600); e != nil {
			t.Fatal(e)
		}
	}
	write("main.ts", "")
	write("base.json", `{"compilerOptions":{"checkers":8,"singleThreaded":true},"files":["main.ts"]}`)
	write("second.json", `{"compilerOptions":{"checkers":4,"singleThreaded":false}}`)
	write("clear.json", `{"compilerOptions":{"checkers":null,"singleThreaded":null}}`)
	write("derived-clear.json", `{"extends":"./clear.json"}`)
	for _, c := range []struct {
		name, config string
		args         []string
	}{
		{"absent", `{"files":["main.ts"]}`, nil},
		{"root", `{"compilerOptions":{"checkers":8,"singleThreaded":true},"files":["main.ts"]}`, nil},
		{"inherited", `{"extends":"./base.json"}`, nil},
		{"own", `{"extends":"./base.json","compilerOptions":{"checkers":2,"singleThreaded":false}}`, nil},
		{"own-null", `{"extends":"./base.json","compilerOptions":{"checkers":null,"singleThreaded":null}}`, nil},
		{"cli-two-false", `{"extends":"./base.json"}`, []string{"--checkers", "2", "--singleThreaded", "false"}},
		{"cli-null", `{"extends":"./base.json"}`, []string{"--checkers", "null"}},
		{"cli-false", `{"extends":"./base.json"}`, []string{"--singleThreaded", "false"}},
		{"cli-repeated-null", `{"extends":"./base.json"}`, []string{"--checkers", "2", "--checkers", "null", "--singleThreaded", "true", "--singleThreaded", "null"}},
		{"array-last", `{"extends":["./base.json","./second.json"]}`, nil},
		{"array-own", `{"extends":["./base.json","./second.json"],"compilerOptions":{"checkers":2,"singleThreaded":true}}`, nil},
		{"array-null", `{"extends":["./base.json","./clear.json"]}`, nil},
		{"array-derived-clear", `{"extends":["./base.json","./derived-clear.json"]}`, nil},
	} {
		write("tsconfig.json", c.config)
		cli := tsoptions.ParseCommandLine(c.args, host)
		wrapped := &collections.OrderedMap[string, any]{}
		wrapped.Set("compilerOptions", cli.Raw)
		parsed, errors := tsoptions.GetParsedCommandLineOfConfigFile(directory+"/tsconfig.json", cli.CompilerOptions(), wrapped, host, nil)
		if len(errors) != 0 {
			t.Fatal(errors)
		}
		codes := []int32{}
		for _, d := range parsed.Errors {
			codes = append(codes, d.Code())
		}
		for _, files := range []int{0, 1, 2, 17} {
			opts := parsed.CompilerOptions()
			p := &Program{opts: ProgramOptions{Config: &tsoptions.ParsedCommandLine{ParsedConfig: &core.ParsedOptions{CompilerOptions: opts}}}, processedFiles: processedFiles{files: make([]*ast.SourceFile, files)}}
			rows = append(rows, row{c.name, opts.Checkers, int(opts.SingleThreaded), files, len(newCheckerPool(p).checkers), codes})
		}
	}
	data, e := json.MarshalIndent(rows, "", "  ")
	if e != nil {
		t.Fatal(e)
	}
	if e = os.WriteFile(os.Getenv("TSR_TRANSPORT_MERGES"), append(data, '\n'), 0600); e != nil {
		t.Fatal(e)
	}
}
