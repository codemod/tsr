package compiler

import (
	"encoding/json"
	"fmt"
	"github.com/microsoft/typescript-go/internal/ast"
	"github.com/microsoft/typescript-go/internal/core"
	"github.com/microsoft/typescript-go/internal/tsoptions"
	"github.com/microsoft/typescript-go/internal/tspath"
	"github.com/microsoft/typescript-go/internal/vfs"
	"github.com/microsoft/typescript-go/internal/vfs/osvfs"
	"os"
	"testing"
)

func selectionPtr(n int) *int { return &n }

func TestTSRWorkerSelectionOracle(t *testing.T) {
	type observation struct {
		Requested *int `json:"requested"`
		Files     int  `json:"files"`
		Single    bool `json:"single"`
		Workers   int  `json:"workers"`
	}
	rows := []observation{}
	for _, files := range []int{0, 1, 2, 3, 4, 5, 8, 17, 256, 1000} {
		for _, requested := range []*int{nil, selectionPtr(-3), selectionPtr(0), selectionPtr(1), selectionPtr(2), selectionPtr(4), selectionPtr(8), selectionPtr(256), selectionPtr(257), selectionPtr(2147483647)} {
			for _, single := range []bool{false, true} {
				options := &core.CompilerOptions{Checkers: requested}
				if single {
					options.SingleThreaded = core.TSTrue
				}
				program := &Program{opts: ProgramOptions{Config: &tsoptions.ParsedCommandLine{ParsedConfig: &core.ParsedOptions{CompilerOptions: options}}}, processedFiles: processedFiles{files: make([]*ast.SourceFile, files)}}
				pool := newCheckerPool(program)
				rows = append(rows, observation{requested, files, single, len(pool.checkers)})
			}
		}
	}
	data, err := json.MarshalIndent(rows, "", "  ")
	if err != nil {
		t.Fatal(err)
	}
	if err = os.WriteFile(os.Getenv("TSR_WORKER_ORACLE"), append(data, '\n'), 0600); err != nil {
		t.Fatal(err)
	}
	fmt.Printf("TSR_WORKER_ORACLE_ROWS=%d\n", len(rows))
}

type selectionHost struct{ directory string }

func (h selectionHost) FS() vfs.FS                  { return osvfs.FS() }
func (h selectionHost) GetCurrentDirectory() string { return h.directory }

func TestTSRWorkerOptionsOracle(t *testing.T) {
	type optionsObservation struct {
		Case     string `json:"case"`
		Checkers *int   `json:"checkers"`
		Single   bool   `json:"single"`
		Errors   int    `json:"errors"`
	}
	directory := t.TempDir()
	host := selectionHost{directory}
	cases := []optionsObservation{}
	config := map[string]any{"compilerOptions": map[string]any{"checkers": 8, "singleThreaded": true}, "files": []any{"main.ts"}}
	text, _ := json.Marshal(config)
	parsedJSON, parseErrors := tsoptions.ParseConfigFileTextToJson(directory+"/tsconfig.json", tspath.Path(directory+"/tsconfig.json"), string(text))
	if len(parseErrors) != 0 {
		t.Fatal(parseErrors)
	}
	root := tsoptions.ParseJsonConfigFileContent(parsedJSON, host, directory, nil, "tsconfig.json", nil, nil, nil)
	cases = append(cases, optionsObservation{"root-config", root.CompilerOptions().Checkers, root.CompilerOptions().SingleThreaded.IsTrue(), len(root.Errors)})
	data, _ := json.Marshal(config)
	if err := os.WriteFile(directory+"/base.json", data, 0600); err != nil {
		t.Fatal(err)
	}
	inheritedJSON, _ := tsoptions.ParseConfigFileTextToJson(directory+"/tsconfig.json", tspath.Path(directory+"/tsconfig.json"), `{"extends":"./base.json"}`)
	inherited := tsoptions.ParseJsonConfigFileContent(inheritedJSON, host, directory, nil, "tsconfig.json", nil, nil, nil)
	cases = append(cases, optionsObservation{"inherited-config", inherited.CompilerOptions().Checkers, inherited.CompilerOptions().SingleThreaded.IsTrue(), len(inherited.Errors)})
	overrideJSON, _ := tsoptions.ParseConfigFileTextToJson(directory+"/tsconfig.json", tspath.Path(directory+"/tsconfig.json"), `{"extends":"./base.json","compilerOptions":{"checkers":2,"singleThreaded":false}}`)
	override := tsoptions.ParseJsonConfigFileContent(overrideJSON, host, directory, nil, "tsconfig.json", nil, nil, nil)
	cases = append(cases, optionsObservation{"inherited-override", override.CompilerOptions().Checkers, override.CompilerOptions().SingleThreaded.IsTrue(), len(override.Errors)})
	for _, count := range []string{"2", "0", "-1", "1.5", "many", "2147483648"} {
		cli := tsoptions.ParseCommandLine([]string{"--checkers", count}, host)
		cases = append(cases, optionsObservation{"cli-" + count, cli.CompilerOptions().Checkers, cli.CompilerOptions().SingleThreaded.IsTrue(), len(cli.Errors)})
	}
	data, err := json.MarshalIndent(cases, "", "  ")
	if err != nil {
		t.Fatal(err)
	}
	if err = os.WriteFile(os.Getenv("TSR_WORKER_OPTIONS_ORACLE"), append(data, '\n'), 0600); err != nil {
		t.Fatal(err)
	}
}
