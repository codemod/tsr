package checker_test

import (
	"encoding/json"
	"github.com/microsoft/typescript-go/internal/ast"
	"github.com/microsoft/typescript-go/internal/bundled"
	"github.com/microsoft/typescript-go/internal/compiler"
	"github.com/microsoft/typescript-go/internal/core"
	"github.com/microsoft/typescript-go/internal/tsoptions"
	"github.com/microsoft/typescript-go/internal/vfs/vfstest"
	"testing"
)

type sharedCase struct {
	Name, Body, Property, Kind string
	Target, Read               bool
	Alias                      string
	DifferentWrite             bool
}

func sharedCases() []sharedCase {
	return []sharedCase{
		{"shared-self", "interface Base { self:this } interface Left extends Base { left:number } interface Right extends Base { right:string } type Both=Left&Right;", "self", "single", true, true, "Both", false},
		{"distinct-self", "interface Left { self:this;left:number } interface Right { self:this;right:string } type Both=Left&Right;", "self", "synthesized", false, true, "", false},
		{"equal-generic-value", "interface Base<T> { value:T } interface Left<U> extends Base<U> { left:U } interface Right<V> extends Base<V> { right:V } type Both=Left<string>&Right<string>;", "value", "single", true, true, "", false},
		{"unequal-generic-value", "interface Base<T> { value:T } interface Left<U> extends Base<U> { left:U } interface Right<V> extends Base<V> { right:V } type Both=Left<string>&Right<number>;", "value", "synthesized", true, false, "", false},
		{"equal-distinct-value", "interface Left { value:string;left:number } interface Right { value:string;right:number } type Both=Left&Right;", "value", "synthesized", false, true, "", false},
		{"optional-generic-value", "interface Base<T> { value?:T } interface Left<U> extends Base<U> { left:U } interface Right<V> extends Base<V> { right:V } type Both=Left<string>&Right<string>;", "value", "single", true, true, "", false},
		{"readonly-generic-value", "interface Base<T> { readonly value:T } interface Left<U> extends Base<U> { left:U } interface Right<V> extends Base<V> { right:V } type Both=Left<string>&Right<string>;", "value", "single", true, true, "", false},
		{"accessor-equal-read-different-write", "class Base<T> { get value():number { return 0; } set value(v:T) {} } class Left<U> extends Base<U> { declare left:U } class Right<V> extends Base<V> { declare right:V } type Both=Left<string>&Right<number>;", "value", "merged-instantiation-clone", true, true, "", true},
		{"accessor-reversed-equal-read-different-write", "class Base<T> { get value():number { return 0; } set value(v:T) {} } class Left<U> extends Base<U> { declare left:U } class Right<V> extends Base<V> { declare right:V } type Both=Right<number>&Left<string>;", "value", "merged-instantiation-clone", true, true, "", true},
		{"equal-read-unequal-generic-args", "interface Base<T> { value:number } interface Left<U> extends Base<U> { left:U } interface Right<V> extends Base<V> { right:V } type Both=Left<string>&Right<number>;", "value", "resolved-scalar", true, true, "", false},
		{"readonly-equal-read-unequal-generic-args", "interface Base<T> { readonly value:number } interface Left<U> extends Base<U> { left:U } interface Right<V> extends Base<V> { right:V } type Both=Left<string>&Right<number>;", "value", "resolved-scalar", true, true, "", false},
		{"private-equal-read-unequal-generic-args", "class Base<T> { private value!:number } class Left<U> extends Base<U> { declare left:U } class Right<V> extends Base<V> { declare right:V } type Both=Left<string>&Right<number>;", "value", "resolved-scalar", true, true, "", false},
		{"private-common", "class Base<T> { private value!:T } class Left<U> extends Base<U> { declare left:U } class Right<V> extends Base<V> { declare right:V } type Both=Left<string>&Right<string>;", "value", "single", true, true, "", false},
	}
}
func TestNativeSharedPropertyParsedPrograms(t *testing.T) {
	for _, tc := range sharedCases() {
		for _, order := range []string{"cold", "expression-first", "checked-first"} {
			t.Run(tc.Name+"/"+order, func(t *testing.T) {
				content := tc.Body + "declare const receiver:Both; const probe=receiver." + tc.Property + ";"
				config := `{"compilerOptions":{"strict":true,"exactOptionalPropertyTypes":true,"target":"es2020","types":[],"skipLibCheck":true,"noEmit":true,"incremental":false,"composite":false,"singleThreaded":true},"files":["input.ts"]}`
				fs := bundled.WrapFS(vfstest.FromMap(map[string]string{"/input.ts": content, "/tsconfig.json": config}, false))
				host := compiler.NewCompilerHost("/", fs, bundled.LibPath(), nil, nil)
				parsed, errors := tsoptions.GetParsedCommandLineOfConfigFile("/tsconfig.json", &core.CompilerOptions{}, nil, host, nil)
				if len(errors) != 0 {
					t.Fatal(errors)
				}
				p := compiler.NewProgram(compiler.ProgramOptions{Config: parsed, Host: host})
				p.BindSourceFiles()
				file := p.GetSourceFile("/input.ts")
				if order == "checked-first" {
					p.GetSemanticDiagnostics(t.Context(), file)
				}
				c, done := p.GetTypeChecker(t.Context())
				defer done()
				var receiver, expression *ast.Node
				for _, statement := range file.Statements.Nodes {
					if statement.Kind != ast.KindVariableStatement {
						continue
					}
					for _, d := range statement.AsVariableStatement().DeclarationList.AsVariableDeclarationList().Declarations.Nodes {
						if d.Name().Kind == ast.KindIdentifier {
							switch d.Name().AsIdentifier().Text {
							case "receiver":
								receiver = d.Name()
							case "probe":
								expression = d.Initializer()
							}
						}
					}
				}
				if receiver == nil || expression == nil {
					t.Fatal("fixture node missing")
				}
				container := c.GetTypeAtLocation(receiver)
				if order == "expression-first" {
					c.GetTypeAtLocation(expression)
				}
				first := c.NativeSharedProperty(container, tc.Property)
				read := c.NativeSharedType(c.GetTypeAtLocation(expression))
				second := c.NativeSharedProperty(container, tc.Property)
				row := map[string]any{"case": tc.Name, "order": order, "content": content, "config": config, "first": first, "second": second, "expression": read}
				encoded, _ := json.Marshal(row)
				t.Logf("NATIVE_SHARED_PROPERTY %s", encoded)
				wantKind := tc.Kind
				// instantiateSymbol can reuse a resolved non-variable scalar.
				// These parsed-program query orders deliberately observe both
				// that original-symbol path and the earlier instantiated clone.
				if wantKind == "resolved-scalar" {
					wantKind = "single"
					if order == "expression-first" {
						wantKind = "merged-instantiation-clone"
					}
				}
				if first.Kind != wantKind || first.SameTarget != tc.Target || first.SameRead != tc.Read {
					t.Fatalf("unexpected native identity: kind=%s target=%v read=%v", first.Kind, first.SameTarget, first.SameRead)
				}
				if len(first.Parts) != 2 || !second.CacheBefore {
					t.Fatal("supplier count or warm publication missing")
				}
				if first.Result.Symbol != second.Result.Symbol || first.Result.Read.ID != second.Result.Read.ID {
					t.Fatal("warm query changed published property")
				}
				if tc.Alias != "" && read.Alias != tc.Alias {
					t.Fatalf("lost native expression alias: %s", read.Alias)
				}
				if first.Kind == "merged-instantiation-clone" {
					if first.Result.Target != first.Result.Symbol || first.Result.LinkTarget != first.Parts[0].Symbol || first.Result.CheckFlags&ast.CheckFlagsInstantiated != 0 || first.Result.Parent != "Base" || first.Result.ParentParameters != 1 {
						t.Fatal("clone parent/root/instantiation flags changed")
					}
					if first.Result.Containing != first.Container.ID || !first.FirstMapperRetained || !first.FirstWriteRetained {
						t.Fatal("merged clone lost containing type, first mapper or first write")
					}
				}
				if tc.DifferentWrite && first.SameWrite {
					t.Fatal("accessor countercontrol must have unequal writes")
				}
				if !first.Parts[0].Readonly && !first.Parts[0].Optional && first.Parts[0].Accessibility == 0 {
					if first.OptionalMismatch != 0 || first.ReadonlyMismatch != 0 {
						t.Fatal("metadata difference must reject equality")
					}
				}
			})
		}
	}
}
