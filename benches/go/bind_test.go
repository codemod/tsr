// The typescript-go side of the parse+bind comparison, mirroring
// crates/tsr-binder/benches/bind.rs.
//
// It lives here rather than in the submodule so the pin stays a clean checkout of
// upstream. To run it:
//
//	cp benches/go/bind_test.go vendor/typescript-go/internal/binder/
//	cd vendor/typescript-go
//	taskset -c 2 go test -run '^$' -bench BenchmarkTsrParseBind -benchmem -cpu 1 -benchtime 2s ./internal/binder/
//	rm internal/binder/bind_test.go
//
// It has to be copied in because `internal/binder` cannot be imported from
// outside its module.
//
// ADR-0009 warns that a Go benchmark we write ourselves is weaker evidence than
// reusing upstream's, because we control both halves. There is no upstream binder
// benchmark — `func Benchmark` across internal/ finds none for binder or checker —
// so this is that case. Read it adversarially.
//
// WHY PARSE AND BIND TOGETHER, rather than timing the bind alone:
// BindSourceFile is idempotent by design. It checks file.IsBound() and returns,
// so a loop that binds the same file repeatedly binds once and then measures a
// boolean check — reporting typescript-go as roughly infinitely fast. Parsing a
// fresh file each iteration removes the possibility rather than working around
// it. The binder's own cost is the difference against the parse-only benchmark,
// which is how `cargo xtask perf` derives it.
package binder

import (
	"testing"

	"github.com/microsoft/typescript-go/internal/ast"
	"github.com/microsoft/typescript-go/internal/core"
	"github.com/microsoft/typescript-go/internal/parser"
	"github.com/microsoft/typescript-go/internal/testutil/fixtures"
	"github.com/microsoft/typescript-go/internal/tspath"
)

func BenchmarkTsrParseBind(b *testing.B) {
	for _, f := range fixtures.BenchFixtures {
		b.Run(f.Name(), func(b *testing.B) {
			f.SkipIfNotExist(b)

			fileName := tspath.GetNormalizedAbsolutePath(f.Path(), "/")
			opts := ast.SourceFileParseOptions{
				FileName: fileName,
				Path:     tspath.ToPath(fileName, "/", true),
			}
			sourceText := f.ReadFile(b)
			scriptKind := core.GetScriptKindFromFileName(fileName)

			for b.Loop() {
				file := parser.ParseSourceFile(opts, sourceText, scriptKind)
				BindSourceFile(file)
			}
		})
	}
}
