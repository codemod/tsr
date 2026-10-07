package testrunner

// Loaded through go's overlay: the pinned vendor checkout is never edited.
import (
 "encoding/hex"
 "fmt"
 "os"
 "path/filepath"
 "slices"
 "strings"
 "sync"
 "testing"

 "github.com/microsoft/typescript-go/internal/ast"
 "github.com/microsoft/typescript-go/internal/core"
 "github.com/microsoft/typescript-go/internal/diagnosticwriter"
 "github.com/microsoft/typescript-go/internal/locale"
 "github.com/microsoft/typescript-go/internal/repo"
 "github.com/microsoft/typescript-go/internal/testutil/harnessutil"
 "github.com/microsoft/typescript-go/internal/testutil/tsbaseline"
)

var fullOracleMu sync.Mutex
func fullOracleHex(s string) string { return hex.EncodeToString([]byte(s)) }
func fullOracleWrite(fields ...string) {
 fullOracleMu.Lock()
 defer fullOracleMu.Unlock()
 f, err := os.OpenFile(os.Getenv("FULL_ORACLE_OUTPUT"), os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0600)
 if err != nil { panic(err) }
 defer f.Close()
 for i, field := range fields {
  if i != 0 { fmt.Fprint(f, "\t") }
  fmt.Fprint(f, fullOracleHex(field))
 }
 fmt.Fprintln(f)
}

// Identity is source tree + suite-relative path (including extension) + native
// configuration description. Never derived from baseline presence or verdict.
func TestFullOracle(t *testing.T) {
 for _, submodule := range []bool{false, true} {
  tree := "local"
  if submodule { tree = "submodule" }
  for _, suite := range []CompilerTestType{TestTypeRegression, TestTypeConformance} {
   runner := NewCompilerBaselineRunner(suite, submodule)
   for _, filename := range runner.EnumerateTestFiles() {
    relative, err := filepath.Rel(filepath.Join(repo.TestDataPath(),runner.basePath), filename)
    if err != nil { t.Fatal(err) }
    caseID := tree + "/" + runner.testSuitName + "/" + filepath.ToSlash(relative)
    t.Run(caseID, func(t *testing.T) {
     published := false
     defer func() {
      if r := recover(); r != nil {
       fullOracleWrite("E", caseID, fmt.Sprintf("discovery panic: %v", r))
       published = true
      }
      if !published { fullOracleWrite("E", caseID, "native configuration discovery failed") }
     }()
     test := getCompilerFileBasedTest(t, filename)
     configs := test.configurations
     if len(configs) == 0 { configs = []*harnessutil.NamedTestConfiguration{nil} }
     // Native map iteration is unspecified. Sort publication by the native name.
     slices.SortFunc(configs, func(a,b *harnessutil.NamedTestConfiguration) int {
      if a == nil || b == nil { return 0 }
      return strings.Compare(a.Name,b.Name)
     })
     for _, config := range configs {
      name := ""
      settings := ""
      if config != nil {
       name = config.Name
       keys := make([]string,0,len(config.Config))
       for key := range config.Config { keys = append(keys,key) }
       slices.Sort(keys)
       for _,key := range keys { settings += key + "=" + fullOracleHex(config.Config[key]) + "\n" }
      }
      id := caseID + "(" + name + ")"
      if os.Getenv("FULL_ORACLE_DISCOVERY") == "1" {
       fullOracleWrite("C", id, filename, name, settings)
       continue
      }
      t.Run(name,func(t *testing.T) {
       completed := false
       defer func() {
        if r := recover(); r != nil {
         fullOracleWrite("R",id,"panic",fmt.Sprint(r),"","","")
         completed = true
        }
        if !completed { fullOracleWrite("R",id,"failed","native compilation aborted","","","") }
       }()
       payload := makeUnitsFromTest(test.content,test.filename)
       c := newCompilerTest(t,id,test.filename,&payload,config)
       files := core.Concatenate(c.tsConfigFiles,core.Concatenate(c.toBeCompiled,c.otherFiles))
       diagnostics := ""
       for _,d := range c.result.Diagnostics { diagnostics += fullOracleDiagnostic(d) + "\n" }
       rendered := "<no content>"
       if len(c.result.Diagnostics) != 0 {
        rendered = tsbaseline.GetErrorBaseline(t,files,diagnosticwriter.WrapASTDiagnostics(c.result.Diagnostics),diagnosticwriter.CompareASTDiagnostics,c.options.Pretty.IsTrue())
       }
       types := "<no content>"
       if !c.harnessOptions.NoTypesAndSymbols {
        allFiles := core.Filter(core.Concatenate(c.toBeCompiled,c.otherFiles),func(f *harnessutil.TestFile) bool { return c.result.Program.GetSourceFile(f.UnitName) != nil })
        // Header is not source-relative: preserve the actual native baseline header.
        parts := strings.Split(filepath.ToSlash(filename),"/tests/")
        header := "tests/" + parts[len(parts)-1]
        types = tsbaseline.FullOracleTypeBaseline(c.result.Program,allFiles,header,len(c.result.Diagnostics)>0)
       }
       // Record the upstream runner's skip policy separately, never omit the run.
       eligibility := "eligible"
       if slices.Contains(skippedTests,filepath.Base(filename)) { eligibility = "native runner skipped filename" }
       t.Run("native eligibility",func(t *testing.T) {
        defer func(){ if t.Skipped() { eligibility = "native runner unsupported options" } }()
        harnessutil.SkipUnsupportedCompilerOptions(t,c.options)
       })
       fullOracleWrite("R",id,"complete",eligibility,diagnostics,rendered,types)
       completed = true
      })
     }
     published = true
    })
   }
  }
 }
}

// Lossless semantic payload, including trees and related information. Native
// positions are byte offsets; UTF-16 offsets are included explicitly as well.
func fullOracleDiagnostic(d *ast.Diagnostic) string {
 file := ""
 start,length := 0,0
 if d.File()!=nil {
  file=d.File().FileName()
  start=int(core.UTF16Len(d.File().Text()[:d.Pos()]))
  length=int(core.UTF16Len(d.File().Text()[d.Pos():d.End()]))
 }
 out := fmt.Sprintf("%s,%d,%d,%d,%d,%d,%d,%s,%s,%t,%t,%t",fullOracleHex(file),d.Pos(),d.Len(),start,length,d.Code(),d.Category(),fullOracleHex(string(d.MessageKey())),fullOracleHex(d.Localize(locale.Default)),d.ReportsUnnecessary(),d.ReportsDeprecated(),d.SkippedOnNoEmit())
 out += ",["
 for _,arg := range d.MessageArgs() { out += fullOracleHex(arg)+";" }
 out += "],["
 for _,child := range d.MessageChain() { out += fullOracleHex(fullOracleDiagnostic(child))+";" }
 out += "],["
 for _,info := range d.RelatedInformation() { out += fullOracleHex(fullOracleDiagnostic(info))+";" }
 return out + "]"
}
