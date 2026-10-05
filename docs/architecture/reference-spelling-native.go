// Archive command inside pinned native cmd/reference-spelling. No vendor edits.
// Selected probes are top-level variable/function declaration names, not alias
// declarations or heritage expressions (which have additional baseline rules).
package main

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"slices"
	"strings"

	"github.com/microsoft/typescript-go/internal/ast"
	"github.com/microsoft/typescript-go/internal/bundled"
	"github.com/microsoft/typescript-go/internal/checker"
	"github.com/microsoft/typescript-go/internal/compiler"
	"github.com/microsoft/typescript-go/internal/core"
	"github.com/microsoft/typescript-go/internal/nodebuilder"
	"github.com/microsoft/typescript-go/internal/printer"
	"github.com/microsoft/typescript-go/internal/testrunner"
	"github.com/microsoft/typescript-go/internal/tsoptions"
	"github.com/microsoft/typescript-go/internal/tspath"
	"github.com/microsoft/typescript-go/internal/vfs/vfstest"
)

type unit struct{ Name, Text string }
type query struct {
	File, Name string
	Node       *ast.Node
	Source     *ast.SourceFile
}
type printRecord struct {
	File       string `json:"file"`
	Name       string `json:"name"`
	Phase      string `json:"phase"`
	Type       string `json:"type"`
	QueryDelta uint32 `json:"query_instantiation_delta"`
	PrintDelta uint32 `json:"print_instantiation_delta"`
}
type orderRecord struct {
	Reverse         bool              `json:"reverse"`
	Types           []printRecord     `json:"types"`
	LoadedInputs    map[string]string `json:"loaded_inputs_sha256"`
	Diagnostics     []map[string]any  `json:"diagnostics"`
	Declarations    map[string]string `json:"declarations"`
	EmitDiagnostics []map[string]any  `json:"emit_diagnostics"`
	EmitSkipped     bool              `json:"emit_skipped"`
}

func digest(text string) string {
	h := sha256.Sum256([]byte(text))
	return hex.EncodeToString(h[:])
}

func diagnosticRecord(d *ast.Diagnostic) map[string]any {
	file := ""
	if d.File() != nil {
		file = d.File().FileName()
	}
	chains := []map[string]any{}
	for _, child := range d.MessageChain() {
		chains = append(chains, diagnosticRecord(child))
	}
	related := []map[string]any{}
	for _, child := range d.RelatedInformation() {
		related = append(related, diagnosticRecord(child))
	}
	return map[string]any{"file": file, "pos": d.Pos(), "end": d.End(), "code": d.Code(), "category": d.Category(),
		"message": d.String(), "chain": chains, "related": related,
		"reports_unnecessary": d.ReportsUnnecessary(), "reports_deprecated": d.ReportsDeprecated()}
}

func diagnosticRecords(ds []*ast.Diagnostic) []map[string]any {
	result := []map[string]any{}
	for _, d := range compiler.SortAndDeduplicateDiagnostics(ds) {
		result = append(result, diagnosticRecord(d))
	}
	return result
}

func render(c *checker.Checker, q query, t *checker.Type, protocol string) string {
	if protocol == "plain" {
		return c.TypeToString(t)
	}
	// Selected-node branch of tsbaseline/type_symbol_baseline.go. These probes
	// have value meaning; alias-name/heritage special cases do not apply.
	if checker.IsTypeAny(t) {
		return t.AsIntrinsicType().IntrinsicName()
	}
	ctx, putCtx := printer.GetEmitContext()
	defer putCtx()
	ctx.Reset()
	builder := checker.NewNodeBuilder(c, ctx)
	flags := checker.TypeFormatFlagsNoTruncation | checker.TypeFormatFlagsAllowUniqueESSymbolType | checker.TypeFormatFlagsGenerateNamesForShadowedTypeParams
	enclosing := q.Node.Parent
	if protocol == "missing-enclosing" {
		enclosing = nil
	}
	if protocol == "missing-flags" {
		flags = 0
	}
	node := builder.TypeToTypeNode(t, enclosing, nodebuilder.Flags(flags&checker.TypeFormatFlagsNodeBuilderFlagsMask)|nodebuilder.FlagsIgnoreErrors, nodebuilder.InternalFlagsAllowUnresolvedNames, nil)
	writer := printer.NewTextWriter("", 0)
	p := printer.NewPrinter(printer.PrinterOptions{RemoveComments: true}, printer.PrintHandlers{}, ctx)
	p.Write(node, q.Source, writer, nil)
	return writer.String()
}

func observe(units []unit, protocol string, reverse bool) orderRecord {
	files := map[string]string{}
	roots := []string{}
	for _, u := range units {
		name := tspath.GetNormalizedAbsolutePath(u.Name, "/")
		if _, exists := files[name]; exists {
			panic("duplicate fixture unit")
		}
		files[name] = u.Text
		roots = append(roots, name)
	}
	config, err := json.Marshal(map[string]any{"compilerOptions": map[string]any{
		"strict": true, "skipLibCheck": true, "target": "esnext", "singleThreaded": true,
		"declaration": true, "emitDeclarationOnly": true, "newLine": "lf"}, "files": roots})
	if err != nil {
		panic(err)
	}
	files["/tsconfig.json"] = string(config)
	fs := bundled.WrapFS(vfstest.FromMap(files, true))
	host := compiler.NewCompilerHost("/", fs, bundled.LibPath(), nil, nil)
	parsed, errors := tsoptions.GetParsedCommandLineOfConfigFile("/tsconfig.json", &core.CompilerOptions{}, nil, host, nil)
	if len(errors) != 0 {
		panic("invalid config")
	}
	p := compiler.NewProgram(compiler.ProgramOptions{Config: parsed, Host: host})
	p.BindSourceFiles()
	c, done := p.GetTypeChecker(context.Background())
	queries := []query{}
	for _, root := range roots {
		file := p.GetSourceFile(root)
		if file == nil {
			panic("missing root")
		}
		for _, stmt := range file.Statements.Nodes {
			nodes := []*ast.Node{}
			if ast.IsVariableStatement(stmt) {
				for _, decl := range stmt.AsVariableStatement().DeclarationList.AsVariableDeclarationList().Declarations.Nodes {
					nodes = append(nodes, decl.Name())
				}
			}
			if ast.IsFunctionDeclaration(stmt) {
				nodes = append(nodes, stmt.Name())
			}
			for _, node := range nodes {
				if node != nil && ast.IsIdentifier(node) && strings.HasPrefix(node.Text(), "probe_") {
					queries = append(queries, query{strings.TrimPrefix(root, "/"), node.Text(), node, file})
				}
			}
		}
	}
	if len(queries) == 0 {
		panic("no queries")
	}
	if reverse {
		slices.Reverse(queries)
	}
	result := orderRecord{Reverse: reverse, Types: []printRecord{}, LoadedInputs: map[string]string{}, Declarations: map[string]string{}}
	for _, phase := range []string{"before-check", "after-check"} {
		if phase == "after-check" {
			// Exact one-checker configuration: queries cannot migrate between owners.
			for _, root := range roots {
				c.GetDiagnostics(context.Background(), p.GetSourceFile(root))
			}
		}
		for _, q := range queries {
			before := c.TotalInstantiationCount
			t := c.GetTypeAtLocation(q.Node)
			afterQuery := c.TotalInstantiationCount
			text := render(c, q, t, protocol)
			result.Types = append(result.Types, printRecord{q.File, q.Name, phase, text,
				afterQuery - before, c.TotalInstantiationCount - afterQuery})
		}
	}
	done()
	// Program APIs acquire the checker again; hold none across diagnostics/emit.
	ds := compiler.GetDiagnosticsOfAnyProgram(context.Background(), p, nil, true, p.GetBindDiagnostics, p.GetSemanticDiagnostics)
	result.Diagnostics = diagnosticRecords(ds)
	emit := p.Emit(context.Background(), compiler.EmitOptions{EmitOnly: compiler.EmitOnlyDts,
		WriteFile: func(name, text string, _ *compiler.WriteFileData) error {
			if _, exists := result.Declarations[name]; exists {
				return fmt.Errorf("duplicate declaration %s", name)
			}
			result.Declarations[name] = text
			return nil
		}})
	if emit == nil {
		panic("missing emit result")
	}
	result.EmitSkipped = emit.EmitSkipped
	result.EmitDiagnostics = diagnosticRecords(emit.Diagnostics)
	for _, file := range p.GetSourceFiles() {
		result.LoadedInputs[file.FileName()] = digest(file.Text())
	}
	return result
}

func main() {
	if len(os.Args) != 3 {
		panic("usage: reference-spelling FIXTURE baseline|plain|missing-enclosing|missing-flags")
	}
	protocol := os.Args[2]
	if !slices.Contains([]string{"baseline", "plain", "missing-enclosing", "missing-flags"}, protocol) {
		panic("unknown protocol")
	}
	text, err := os.ReadFile(os.Args[1])
	if err != nil {
		panic(err)
	}
	units, _, _, _, err := testrunner.ParseTestFilesAndSymlinks(string(text), "contract.ts", func(name, text string, _ map[string]string) (unit, error) { return unit{name, text}, nil })
	if err != nil {
		panic(err)
	}
	orders := []orderRecord{}
	for _, reverse := range []bool{false, true} {
		orders = append(orders, observe(units, protocol, reverse))
	}
	if err := json.NewEncoder(os.Stdout).Encode(map[string]any{"schema": 1, "protocol": protocol, "units": units, "orders": orders}); err != nil {
		panic(err)
	}
}
