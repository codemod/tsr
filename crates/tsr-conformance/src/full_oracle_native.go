// Installed only through a Go overlay; the pinned native checkout is never modified.
package testrunner

import (
	"encoding/hex"
	"fmt"
	"github.com/microsoft/typescript-go/internal/ast"
	"github.com/microsoft/typescript-go/internal/diagnosticwriter"
	"github.com/microsoft/typescript-go/internal/locale"
	"github.com/microsoft/typescript-go/internal/testutil/harnessutil"
	"github.com/microsoft/typescript-go/internal/testutil/tsbaseline"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"testing"
)

func oracleHex(s string) string { return hex.EncodeToString([]byte(s)) }

func TestFullOracle(t *testing.T) {
	mode := os.Getenv("TSR_ORACLE_MODE")
	if mode == "" {
		return
	}
	root := os.Getenv("TSR_ORACLE_CORPUS")
	if mode == "plan" {
		out, err := os.Create(os.Getenv("TSR_ORACLE_OUTPUT"))
		if err != nil {
			t.Fatal(err)
		}
		defer out.Close()
		var paths []string
		for _, suite := range []string{"compiler", "conformance"} {
			err := filepath.WalkDir(filepath.Join(root, suite), func(p string, d os.DirEntry, err error) error {
				if err != nil {
					return err
				}
				if !d.IsDir() && (strings.HasSuffix(p, ".ts") || strings.HasSuffix(p, ".tsx")) {
					paths = append(paths, p)
				}
				return nil
			})
			if err != nil {
				t.Fatal(err)
			}
		}
		sort.Strings(paths)
		for _, p := range paths {
			emitted := false
			t.Run(filepath.Base(p), func(t *testing.T) {
				defer func() {
					if r := recover(); r != nil {
						t.Errorf("enumeration panic: %v", r)
					}
				}()
				test := getCompilerFileBasedTest(t, p)
				if len(test.configurations) == 0 {
					fmt.Fprintf(out, "%s\t\tCOMPLETE\n", oracleHex(p))
					emitted = true
					return
				}
				sort.Slice(test.configurations, func(i, j int) bool { return test.configurations[i].Name < test.configurations[j].Name })
				for _, c := range test.configurations {
					fmt.Fprintf(out, "%s\t%s\tCOMPLETE\n", oracleHex(p), oracleHex(c.Name))
					emitted = true
				}
			})
			if !emitted {
				fmt.Fprintf(out, "%s\t\tDISCOVERY_FAILED\n", oracleHex(p))
			}
		}
		fmt.Fprintln(out, "COMPLETE")
		return
	}
	p := os.Getenv("TSR_ORACLE_CASE")
	name := os.Getenv("TSR_ORACLE_VARIANT")
	test := getCompilerFileBasedTest(t, p)
	payload := makeUnitsFromTest(test.content, p)
	var selected = -1
	for i, c := range test.configurations {
		if c.Name == name {
			selected = i
			break
		}
	}
	var c *compilerTest
	if selected >= 0 {
		c = newCompilerTest(t, "oracle", p, &payload, test.configurations[selected])
	} else if len(test.configurations) == 0 && name == "" {
		c = newCompilerTest(t, "oracle", p, &payload, nil)
	} else {
		t.Fatal("configuration missing")
	}
	out, err := os.Create(os.Getenv("TSR_ORACLE_OUTPUT"))
	if err != nil {
		t.Fatal(err)
	}
	defer out.Close()
	// The exact expanded native configuration is a request, not expected type selection.
	if selected >= 0 {
		keys := make([]string, 0, len(test.configurations[selected].Config))
		for k := range test.configurations[selected].Config {
			keys = append(keys, k)
		}
		sort.Strings(keys)
		for _, k := range keys {
			fmt.Fprintf(out, "O\t%s\t%s\n", oracleHex(k), oracleHex(test.configurations[selected].Config[k]))
		}
	}
	for _, d := range c.result.Diagnostics {
		file := ""
		start, length := d.Pos(), d.Len()
		if d.File() != nil {
			file = d.File().FileName()
			text := d.File().Text()
			start = oracleUTF16(text[:d.Pos()])
			length = oracleUTF16(text[d.Pos():d.End()])
		}
		message := diagnosticwriter.FlattenDiagnosticMessage(diagnosticwriter.WrapASTDiagnostic(d), "\n", locale.Default)
		fmt.Fprintf(out, "D\t%s\t%d\t%d\t%d\t%d\t%s\n", oracleHex(oraclePath(file)), start, length, d.Code(), d.Category(), oracleHex(message))
		oracleDetails(out, d, "head")
	}
	files := append(append([]*harnessutil.TestFile{}, c.toBeCompiled...), c.otherFiles...)
	for _, row := range tsbaseline.FullOracleTypes(c.result.Program, files, len(c.result.Diagnostics) > 0) {
		fmt.Fprintf(out, "T\t%s\t%s\t%s\n", oracleHex(oraclePath(row[0])), oracleHex(row[1]), oracleHex(row[2]))
	}
	fmt.Fprintln(out, "COMPLETE")
}

// Emit metadata and recursive chain/related records in native publication order.
// Paths encode list identity and position; equal messages do not collapse separate records.
func oracleDetails(out *os.File, d *ast.Diagnostic, path string) {
	fmt.Fprintf(out, "M\t%s\t%t\t%t\t%t\n", path, d.ReportsUnnecessary(), d.ReportsDeprecated(), d.SkippedOnNoEmit())
	for _, list := range []struct {
		tag  string
		rows []*ast.Diagnostic
	}{{"C", d.MessageChain()}, {"R", d.RelatedInformation()}} {
		for i, child := range list.rows {
			childPath := fmt.Sprintf("%s/%s%d", path, list.tag, i)
			file := ""
			start, length := child.Pos(), child.Len()
			if child.File() != nil {
				file = child.File().FileName()
				text := child.File().Text()
				start = oracleUTF16(text[:child.Pos()])
				length = oracleUTF16(text[child.Pos():child.End()])
			}
			fmt.Fprintf(out, "%s\t%s\t%s\t%d\t%d\t%d\t%d\t%s\n", list.tag, childPath, oracleHex(oraclePath(file)), start, length, child.Code(), child.Category(), oracleHex(child.Localize(locale.Default)))
			oracleDetails(out, child, childPath)
		}
	}
}

func oraclePath(s string) string {
	return strings.ReplaceAll(strings.ReplaceAll(strings.ReplaceAll(s, "/.src/", ""), "/.lib/", ""), "/.ts/", "")
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
