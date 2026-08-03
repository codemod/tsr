// The typescript-go side of the peak-RSS comparison, mirroring
// crates/tsr-parser/examples/rss.rs so the two figures are comparable.
//
// It lives here rather than in the submodule so the pin stays a clean checkout of
// upstream. To run it:
//
//	cp benches/go/rss_test.go vendor/typescript-go/internal/parser/
//	cd vendor/typescript-go
//	taskset -c 2 go test -run TestTsrPeakRSS -v ./internal/parser/
//	rm internal/parser/rss_test.go
//
// It has to be copied in because `internal/parser` cannot be imported from
// outside its module.
//
// ADR-0009 says a benchmark we write for the Go side is weaker evidence than
// reusing upstream's, because we control both halves of the comparison. There is
// no upstream RSS benchmark, so this is that case: read it as an adversarial
// artifact. The things to check are that it measures VmHWM rather than VmRSS,
// that the baseline is taken after the source text is read on both sides, and
// that it holds every parsed file alive to the measurement.
package parser

import (
	"fmt"
	"os"
	"runtime"
	"strconv"
	"strings"
	"testing"

	"github.com/microsoft/typescript-go/internal/ast"
	"github.com/microsoft/typescript-go/internal/core"
	"github.com/microsoft/typescript-go/internal/testutil/fixtures"
	"github.com/microsoft/typescript-go/internal/tspath"
)

// peakRSSKiB reads VmHWM, the kernel's high-water mark, exactly as the Rust side
// does. VmRSS would depend on when the collector last ran.
func peakRSSKiB(t *testing.T) uint64 {
	status, err := os.ReadFile("/proc/self/status")
	if err != nil {
		t.Fatal(err)
	}
	for _, line := range strings.Split(string(status), "\n") {
		if rest, ok := strings.CutPrefix(line, "VmHWM:"); ok {
			fields := strings.Fields(rest)
			v, err := strconv.ParseUint(fields[0], 10, 64)
			if err != nil {
				t.Fatal(err)
			}
			return v
		}
	}
	t.Fatal("no VmHWM")
	return 0
}

func TestTsrPeakRSS(t *testing.T) {
	type loaded struct {
		opts ast.SourceFileParseOptions
		text string
		kind core.ScriptKind
	}
	var inputs []loaded
	var bytes int
	// Same four fixtures as the Rust side; empty.ts is excluded there too since
	// it contributes nothing to memory.
	want := map[string]bool{
		"checker.ts": true, "dom.generated.d.ts": true,
		"Herebyfile.mjs": true, "jsxComplexSignatureHasApplicabilityError.tsx": true,
	}
	for _, f := range fixtures.BenchFixtures {
		if !want[f.Name()] {
			continue
		}
		f.SkipIfNotExist(t)
		fileName := tspath.GetNormalizedAbsolutePath(f.Path(), "/")
		text := f.ReadFile(t)
		bytes += len(text)
		inputs = append(inputs, loaded{
			opts: ast.SourceFileParseOptions{FileName: fileName, Path: tspath.ToPath(fileName, "/", true)},
			text: text,
			kind: core.GetScriptKindFromFileName(fileName),
		})
	}

	baseline := peakRSSKiB(t)

	files := make([]*ast.SourceFile, 0, len(inputs))
	nodes := 0
	for _, in := range inputs {
		f := ParseSourceFile(in.opts, in.text, in.kind)
		nodes += f.NodeCount
		files = append(files, f)
	}

	peak := peakRSSKiB(t)
	runtime.KeepAlive(files)

	fmt.Printf("files:        %d\n", len(inputs))
	fmt.Printf("source bytes: %d\n", bytes)
	fmt.Printf("nodes:        %d\n", nodes)
	fmt.Printf("baseline KiB: %d\n", baseline)
	fmt.Printf("loaded KiB:   %d\n", peak)
	fmt.Printf("ast KiB:      %d\n", peak-baseline)
	fmt.Printf("bytes per source byte: %.2f\n", float64((peak-baseline)*1024)/float64(bytes))
}
