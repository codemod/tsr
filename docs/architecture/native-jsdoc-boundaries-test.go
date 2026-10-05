package parser_test

import (
	"sync"
	"testing"

	"github.com/microsoft/typescript-go/internal/ast"
	"github.com/microsoft/typescript-go/internal/core"
	"github.com/microsoft/typescript-go/internal/parser"
	"github.com/microsoft/typescript-go/internal/tspath"
)

func TestJSDocCostNativeEagerBoundary(t *testing.T) {
	for _, tc := range []struct {
		name, text string
		kind       core.ScriptKind
		eager      bool
		deprecated bool
	}{
		{"ts-ordinary", "/** ordinary docs */ export interface I {}", core.ScriptKindTS, false, false},
		{"tsx-ordinary", "/** ordinary docs */ export interface I {}", core.ScriptKindTSX, false, false},
		{"declaration", "/** ordinary docs */ export interface I {}", core.ScriptKindTS, false, false},
		{"deprecated", "/** @deprecated use another */ export interface I {}", core.ScriptKindTS, false, true},
		{"link", "/** {@link I} */ export interface I {}", core.ScriptKindTS, true, false},
		{"see", "/** @see I */ export interface I {}", core.ScriptKindTS, true, false},
		{"js", "/** @param {number} x */ function f(x) {}", core.ScriptKindJS, true, false},
	} {
		t.Run(tc.name, func(t *testing.T) {
			name := "/index.ts"
			if tc.name == "declaration" {
				name = "/index.d.ts"
			}
			file := parser.ParseSourceFile(ast.SourceFileParseOptions{FileName: name, Path: tspath.Path(name)}, tc.text, tc.kind)
			node := file.Statements.Nodes[0]
			if node.Flags&ast.NodeFlagsHasJSDoc == 0 {
				t.Fatal("missing cheap documentation flag")
			}
			if got := node.Flags&ast.NodeFlagsPossiblyContainsDeprecatedTag != 0; got != tc.deprecated {
				t.Fatalf("deprecated flag %v, want %v", got, tc.deprecated)
			}
			if got := len(node.EagerJSDoc(file)) != 0; got != tc.eager {
				t.Fatalf("initial eager state %v, want %v", got, tc.eager)
			}
			first := node.JSDoc(file)
			if len(first) != 1 || first[0].Parent != node {
				t.Fatal("query must parse exactly one comment with its native host parent")
			}
			if again := node.JSDoc(file); len(again) != 1 || again[0] != first[0] {
				t.Fatal("repeated query replaced documentation identity")
			}
			if eager := node.EagerJSDoc(file); len(eager) != 1 || eager[0] != first[0] {
				t.Fatal("completed query was not published in file cache")
			}
		})
	}
}

func TestJSDocCostNativeJSReparseDiscovery(t *testing.T) {
	for _, tc := range []struct {
		name, text string
		kind       ast.Kind
	}{
		{"import", "/** @import {Item} from './base' */\nexport const item = 1;", ast.KindJSImportDeclaration},
		{"typedef-template", "/** @template T\n * @typedef {{value:T}} Box */\nconst anchor = 1;", ast.KindJSTypeAliasDeclaration},
		{"overload", "/** @overload\n * @param {string} x\n * @returns {string} */\n/** @param {string|number} x */\nfunction f(x) {return x}", ast.KindFunctionDeclaration},
	} {
		t.Run(tc.name, func(t *testing.T) {
			file := parser.ParseSourceFile(ast.SourceFileParseOptions{FileName: "/index.js", Path: "/index.js"}, tc.text, core.ScriptKindJS)
			found := false
			for _, node := range file.Statements.Nodes {
				if node.Kind == tc.kind && node.Flags&ast.NodeFlagsReparsed != 0 {
					found = true
					if tc.name == "typedef-template" && len(node.TypeParameters()) != 1 {
						t.Fatal("template parameter missing on reparsed alias")
					}
				}
			}
			if !found {
				t.Fatal("required synthetic declaration absent before binding")
			}
			if tc.name == "import" && (len(file.Imports()) != 1 || file.Imports()[0].Text() != "./base") {
				t.Fatal("JSDoc import missing from loader references")
			}
		})
	}
}

func TestJSDocCostNativeConcurrentQueryPublication(t *testing.T) {
	file := parser.ParseSourceFile(ast.SourceFileParseOptions{FileName: "/index.ts", Path: "/index.ts"}, "/** ordinary */ export interface I {}", core.ScriptKindTS)
	node := file.Statements.Nodes[0]
	const workers = 8
	answers := make([]*ast.Node, workers)
	var group sync.WaitGroup
	for i := range answers {
		group.Add(1)
		go func(i int) {
			defer group.Done()
			answers[i] = node.JSDoc(file)[0]
		}(i)
	}
	group.Wait()
	for _, answer := range answers {
		if answer != answers[0] {
			t.Fatal("concurrent queries published different identities")
		}
	}
}
