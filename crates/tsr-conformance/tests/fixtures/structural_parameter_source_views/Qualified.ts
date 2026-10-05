// @target: es2015
// @strict: true
// @module: commonjs
// @filename: entry.ts
export type ModuleInputs<F extends (...args: any[]) => any> = F extends (...args: infer P) => any ? P : never;
export function peer(value: number) { return value; }
export function otherPeer(value: string, count: number) { return value; }
export function moduleFoo(value: ModuleInputs<typeof peer>[0]) { return value; }
export function moduleBar(value: ModuleInputs<typeof otherPeer>[0]) { return value; }
// @filename: named.ts
import { moduleFoo, moduleBar, ModuleInputs as RenamedInputs, peer as renamedPeer, otherPeer as renamedOtherPeer } from "./entry";
const renamedNumberView = moduleFoo;
const renamedStringView = moduleBar;
function shadowTypeAlias() {
    type RenamedInputs = boolean;
    const shadowedTypeView = moduleFoo;
    return shadowedTypeView;
}
function shadowValueAlias() {
    const renamedPeer = (value: boolean) => value;
    const shadowedValueView = moduleFoo;
    return shadowedValueView;
}
// @filename: namespace.ts
import * as EntryNS from "./entry";
import { moduleFoo } from "./entry";
const namespaceNumberView = EntryNS.moduleFoo;
const namespaceStringView = EntryNS.moduleBar;
function shadowNamespaceAlias() {
    const EntryNS = { peer: (value: boolean) => value };
    const shadowedNamespaceView = moduleFoo;
    return shadowedNamespaceView;
}
// @filename: competing.ts
import * as FirstNS from "./entry";
import * as LaterNS from "./entry";
const competingNumberView = LaterNS.moduleFoo;
const competingStringView = FirstNS.moduleBar;
// @filename: absent.ts
import { moduleFoo, moduleBar } from "./entry";
const importedNumberView = moduleFoo;
const importedStringView = moduleBar;
// @filename: shortest.ts
import * as EarlierNS from "./entry";
import { ModuleInputs as LaterInputs, peer as laterPeer, moduleFoo } from "./entry";
const shortestView = EarlierNS.moduleFoo;
// @filename: spelling.ts
import { ModuleInputs as EarlierInputs, peer as earlierPeer } from "./entry";
import { ModuleInputs, peer, moduleFoo } from "./entry";
const preservedView = moduleFoo;
