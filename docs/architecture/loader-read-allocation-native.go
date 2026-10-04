// Standalone pinned-native physical-reader oracle. Build under cmd/ in the
// pinned module so the normal internal/osvfs implementation is exercised.
package main

import (
	"encoding/hex"
	"fmt"
	"os"

	"github.com/microsoft/typescript-go/internal/vfs/osvfs"
)

func main() {
	if len(os.Args) != 2 {
		panic("usage: read-allocation /absolute/input.ts")
	}
	text, ok := osvfs.FS().ReadFile(os.Args[1])
	if !ok {
		fmt.Println("missing")
		return
	}
	fmt.Printf("text\t%s\n", hex.EncodeToString([]byte(text)))
}
