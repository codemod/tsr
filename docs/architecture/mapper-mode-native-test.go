// Copy into a task-owned archive with native-mapper-identity-test.go installed.
package checker

import "testing"

func TestMapperModeUnmappedIdentity(t *testing.T) {
	c := identityChecker()
	target, retained := identityParameter(c, "T"), identityParameter(c, "U")
	m := newSimpleTypeMapper(target, c.stringType)
	if c.instantiateType(retained, m) != retained {
		t.Fatal("native substitution must retain a parameter absent from the mapper")
	}
	if c.instantiateType(target, m) != c.stringType {
		t.Fatal("the mapped parameter must still resolve to its image")
	}
	if c.findActiveMapper(m) != -1 {
		t.Fatal("top-level identity substitutions must release their active mapper")
	}
}
