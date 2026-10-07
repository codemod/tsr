package tsbaseline

import (
 "github.com/microsoft/typescript-go/internal/compiler"
 "github.com/microsoft/typescript-go/internal/testutil/harnessutil"
)

// Overlay-only access to the pinned native full walker; no copied algorithm.
func FullOracleTypeBaseline(program compiler.ProgramLike, files []*harnessutil.TestFile, header string, hadErrors bool) string {
 return generateBaseline(files,newTypeWriterWalker(program,hadErrors),header,false)
}
