// @strict: true
// @noImplicitAny: false
// @allowJs: true
// @checkJs: true
// @module: commonjs
// @noEmit: true
// @filename: /mod.js
Object.defineProperty(exports, "value", { value: 42, writable: true });
Object.defineProperty(module.exports, "readonly", { get() { return "read"; } });
Object.defineProperty(exports, "setonly", {
    /** @param {boolean} v */
    set(v) {}
});
// @filename: /use.ts
import mod = require("./mod");
mod.value;
mod.readonly;
mod.setonly;
mod.value = 2;
mod.readonly = "write";
mod.setonly = false;
