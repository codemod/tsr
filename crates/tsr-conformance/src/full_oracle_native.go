// Native producer for the exact full-corpus oracle (docs/parity/notes/oracle.md).
//
// Installed only through a `go test -overlay` as internal/testrunner/full_oracle_test.go;
// the pinned native checkout is never modified. It reuses the pinned runner's own
// enumeration (CompilerBaselineRunner.EnumerateTestFiles, skippedTests),
// configuration expansion (getCompilerFileBasedTest), compilation (newCompilerTest)
// and harness skip policy (SkipUnsupportedCompilerOptions), so the population and
// every published record come from runSingleConfigTest's own steps. Nothing reads a
// committed baseline.
package testrunner

import (
	"encoding/hex"
	"fmt"
	"os"
	"slices"
	"sort"
	"strings"
	"testing"

	"github.com/microsoft/typescript-go/internal/ast"
	"github.com/microsoft/typescript-go/internal/diagnosticwriter"
	"github.com/microsoft/typescript-go/internal/locale"
	"github.com/microsoft/typescript-go/internal/testutil/harnessutil"
	"github.com/microsoft/typescript-go/internal/testutil/tsbaseline"
	"github.com/microsoft/typescript-go/internal/tspath"
)

func oracleHex(s string) string { return hex.EncodeToString([]byte(s)) }

// The corpus-relative identity, e.g. `compiler/foo.ts`.
func oracleIdentity(p string) string {
	const marker = "/tests/cases/"
	if i := strings.LastIndex(p, marker); i >= 0 {
		return p[i+len(marker):]
	}
	return p
}

func TestFullOracle(t *testing.T) {
	mode := os.Getenv("TSR_ORACLE_MODE")
	if mode == "" {
		t.Skip("TSR_ORACLE_MODE unset")
	}
	out, err := os.Create(os.Getenv("TSR_ORACLE_OUTPUT"))
	if err != nil {
		t.Fatal(err)
	}
	defer out.Close()
	switch mode {
	case "plan":
		oraclePlan(t, out)
	case "case":
		oracleCase(t, out, os.Getenv("TSR_ORACLE_CASE"), os.Getenv("TSR_ORACLE_VARIANT"))
	default:
		t.Fatalf("unknown TSR_ORACLE_MODE %q", mode)
	}
}

// runCompilerTests order: the regression (compiler) runner, then conformance; each
// in EnumerateFiles order; a source's configurations sorted by name.
func oraclePlan(t *testing.T, out *os.File) {
	for _, runner := range []*CompilerBaselineRunner{
		NewCompilerBaselineRunner(TestTypeRegression, true),
		NewCompilerBaselineRunner(TestTypeConformance, true),
	} {
		for _, p := range runner.EnumerateTestFiles() {
			id := oracleHex(oracleIdentity(p))
			if slices.Contains(skippedTests, tspath.GetBaseFileName(p)) {
				fmt.Fprintf(out, "LISTED_SKIP\t%s\n", id)
				continue
			}
			var rows []string
			published := false
			t.Run(tspath.GetBaseFileName(p), func(t *testing.T) {
				defer func() {
					if r := recover(); r != nil {
						t.Errorf("discovery panic: %v", r)
					}
				}()
				test := getCompilerFileBasedTest(t, p)
				if len(test.configurations) == 0 {
					rows = append(rows, fmt.Sprintf("CASE\t%s\t\t\n", id))
				}
				for _, c := range test.configurations {
					keys := make([]string, 0, len(c.Config))
					for k := range c.Config {
						keys = append(keys, k)
					}
					sort.Strings(keys)
					options := make([]string, 0, len(keys))
					for _, k := range keys {
						options = append(options, oracleHex(k)+"="+oracleHex(c.Config[k]))
					}
					rows = append(rows, fmt.Sprintf("CASE\t%s\t%s\t%s\n", id, oracleHex(c.Name), strings.Join(options, ",")))
				}
				// GetFileBasedTestConfigurations builds the varying options from a map
				// range, so its order is random per process; the set is not. Sorting
				// by configuration name makes the plan (and its SHA-256) reproducible.
				sort.Strings(rows)
				published = true
			})
			// A t.Fatal inside configuration expansion publishes no configuration:
			// the source is one explicit failed row, never a guessed default.
			if !published {
				fmt.Fprintf(out, "DISCOVERY_FAILED\t%s\n", id)
				continue
			}
			for _, row := range rows {
				fmt.Fprint(out, row)
			}
		}
	}
	fmt.Fprintln(out, "COMPLETE")
}

func oracleCase(t *testing.T, out *os.File, p string, name string) {
	test := getCompilerFileBasedTest(t, p)
	payload := makeUnitsFromTest(test.content, p)
	var config *harnessutil.NamedTestConfiguration
	for _, c := range test.configurations {
		if c.Name == name {
			config = c
			break
		}
	}
	if config == nil && !(len(test.configurations) == 0 && name == "") {
		t.Fatal("configuration missing")
	}
	c := newCompilerTest(t, "oracle", p, &payload, config)
	// runSingleConfigTest skips the configuration after compiling it.
	supported := false
	t.Run("supported", func(t *testing.T) {
		harnessutil.SkipUnsupportedCompilerOptions(t, c.options)
		supported = true
	})
	if !supported {
		fmt.Fprintln(out, "NATIVE_SKIPPED")
		fmt.Fprintln(out, "COMPLETE")
		return
	}
	// verifyDiagnostics: c.result.Diagnostics in published (sorted) order.
	for _, d := range c.result.Diagnostics {
		file, start, length := oracleSpan(d)
		message := diagnosticwriter.FlattenDiagnosticMessage(diagnosticwriter.WrapASTDiagnostic(d), "\n", locale.Default)
		fmt.Fprintf(out, "D\t%s\t%s\t%s\t%d\t%d\t%s\n", oracleHex(file), start, length, d.Code(), d.Category(), oracleHex(message))
		oracleDetails(out, d, "head")
	}
	// verifyTypesAndSymbols: skipped by @noTypesAndSymbols; otherwise
	// toBeCompiled ++ otherFiles filtered to loaded program files, walked with
	// hadErrorBaseline = len(c.result.Diagnostics) > 0.
	if !c.harnessOptions.NoTypesAndSymbols {
		program := c.result.Program
		var files []string
		for _, f := range append(append([]*harnessutil.TestFile{}, c.toBeCompiled...), c.otherFiles...) {
			if program.GetSourceFile(f.UnitName) != nil {
				files = append(files, f.UnitName)
			}
		}
		for _, row := range tsbaseline.FullOracleTypes(program, files, len(c.result.Diagnostics) > 0) {
			if row.Line < 0 {
				fmt.Fprintf(out, "S\t%s\n", oracleHex(tsbaseline.FullOraclePath(row.File)))
				continue
			}
			fmt.Fprintf(out, "T\t%s\t%d\t%s\t%s\n", oracleHex(tsbaseline.FullOraclePath(row.File)), row.Line, oracleHex(row.Text), oracleHex(row.Type))
		}
	}
	fmt.Fprintln(out, "COMPLETE")
}

// A file-less diagnostic prints no position anywhere (error baseline, CLI), and
// its Loc is a construction detail (-1 or 0), so its span is published as "-".
func oracleSpan(d *ast.Diagnostic) (string, string, string) {
	if d.File() == nil {
		return "", "-", "-"
	}
	text := d.File().Text()
	return tsbaseline.FullOraclePath(d.File().FileName()), fmt.Sprint(oracleUTF16(text[:d.Pos()])), fmt.Sprint(oracleUTF16(text[d.Pos():d.End()]))
}

// Metadata and recursive chain/related records in native publication order.
// Paths encode list identity and position; equal messages never collapse records.
func oracleDetails(out *os.File, d *ast.Diagnostic, path string) {
	fmt.Fprintf(out, "M\t%s\t%t\t%t\t%t\n", path, d.ReportsUnnecessary(), d.ReportsDeprecated(), d.SkippedOnNoEmit())
	for _, list := range []struct {
		tag  string
		rows []*ast.Diagnostic
	}{{"C", d.MessageChain()}, {"R", d.RelatedInformation()}} {
		for i, child := range list.rows {
			childPath := fmt.Sprintf("%s/%s%d", path, list.tag, i)
			file, start, length := oracleSpan(child)
			fmt.Fprintf(out, "%s\t%s\t%s\t%s\t%s\t%d\t%d\t%s\n", list.tag, childPath, oracleHex(file), start, length, child.Code(), child.Category(), oracleHex(child.Localize(locale.Default)))
			oracleDetails(out, child, childPath)
		}
	}
}

func oracleUTF16(s string) int {
	n := 0
	for _, r := range s {
		n++
		if r > 0xffff {
			n++
		}
	}
	return n
}
