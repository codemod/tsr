#!/usr/bin/env python3
"""Generate the public whole-project benchmark `benches/projects/domain-model`.

The project is deterministic: the same `--modules` count always produces the
same bytes, so the committed tree can be regenerated and diffed. It exercises
the checker on ordinary application code rather than on library declarations:
cross-module imports, generic interfaces and classes, abstract members,
discriminated unions narrowed by `switch`, mapped and conditional types,
keyof/indexed access, overloads, optional chaining, and async functions.

Exactly one intentional assignment error is emitted (in `main.ts`) so a run
that silently stops checking changes the diagnostic fingerprint.

    python3 scripts/generate_perf_project.py [--modules N] [--out DIR]
"""

from __future__ import annotations

import argparse
import json
import pathlib
import shutil

DEFAULT_MODULES = 40
DEFAULT_OUT = pathlib.Path(__file__).resolve().parent.parent / "benches/projects/domain-model"

CORE = """\
// Shared building blocks for every generated domain module.

export type Id<Brand extends string> = string & { readonly __brand: Brand };

export interface Entity<Brand extends string> {
  readonly id: Id<Brand>;
  readonly createdAt: number;
}

export type Result<T, E = string> =
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly error: E };

export function ok<T>(value: T): Result<T, never> {
  return { ok: true, value };
}

export function fail<E>(error: E): Result<never, E> {
  return { ok: false, error };
}

export function unwrapOr<T, E>(result: Result<T, E>, fallback: T): T {
  return result.ok ? result.value : fallback;
}

export function mapResult<T, U, E>(result: Result<T, E>, map: (value: T) => U): Result<U, E> {
  if (result.ok) {
    return ok(map(result.value));
  }
  return result;
}

export type DeepReadonly<T> = T extends (infer U)[]
  ? ReadonlyArray<DeepReadonly<U>>
  : T extends object
    ? { readonly [K in keyof T]: DeepReadonly<T[K]> }
    : T;

export type Patch<T> = { [K in keyof T]?: T[K] extends object ? Patch<T[K]> : T[K] };

export type KeysOfType<T, V> = { [K in keyof T]-?: T[K] extends V ? K : never }[keyof T];

export interface Repository<T extends Entity<B>, B extends string> {
  get(id: Id<B>): T | undefined;
  list(filter?: (item: T) => boolean): T[];
  save(item: T): Result<T>;
  remove(id: Id<B>): boolean;
}

export class MemoryRepository<T extends Entity<B>, B extends string> implements Repository<T, B> {
  private readonly items = new Map<Id<B>, T>();

  get(id: Id<B>): T | undefined {
    return this.items.get(id);
  }

  list(filter?: (item: T) => boolean): T[] {
    const all = Array.from(this.items.values());
    return filter ? all.filter(filter) : all;
  }

  save(item: T): Result<T> {
    if (item.id.length === 0) {
      return fail("empty id");
    }
    this.items.set(item.id, item);
    return ok(item);
  }

  remove(id: Id<B>): boolean {
    return this.items.delete(id);
  }

  get size(): number {
    return this.items.size;
  }
}

export abstract class Service<T extends Entity<B>, B extends string> {
  protected constructor(protected readonly repository: Repository<T, B>) {}

  abstract validate(item: T): string[];

  create(item: T): Result<T, string[]> {
    const problems = this.validate(item);
    if (problems.length > 0) {
      return fail(problems);
    }
    const saved = this.repository.save(item);
    return saved.ok ? ok(saved.value) : fail([saved.error]);
  }

  find(predicate: (item: T) => boolean): T | undefined {
    return this.repository.list(predicate)[0];
  }
}

export class EventBus<Events extends Record<string, unknown>> {
  private readonly handlers: { [K in keyof Events]?: Array<(payload: Events[K]) => void> } = {};

  on<K extends keyof Events>(event: K, handler: (payload: Events[K]) => void): () => void {
    const list = this.handlers[event] ?? [];
    list.push(handler);
    this.handlers[event] = list;
    return () => {
      const current = this.handlers[event];
      if (current) {
        this.handlers[event] = current.filter((candidate) => candidate !== handler);
      }
    };
  }

  emit<K extends keyof Events>(event: K, payload: Events[K]): number {
    const list = this.handlers[event];
    if (!list) {
      return 0;
    }
    for (const handler of list) {
      handler(payload);
    }
    return list.length;
  }
}

export function groupBy<T, K extends PropertyKey>(items: readonly T[], key: (item: T) => K): Record<K, T[]> {
  const groups = {} as Record<K, T[]>;
  for (const item of items) {
    const group = key(item);
    (groups[group] ??= []).push(item);
  }
  return groups;
}

export function pick<T, K extends keyof T>(value: T, ...keys: K[]): Pick<T, K> {
  const result = {} as Pick<T, K>;
  for (const key of keys) {
    result[key] = value[key];
  }
  return result;
}

export function format(value: string): string;
export function format(value: number, digits?: number): string;
export function format(value: string | number, digits = 2): string {
  return typeof value === "number" ? value.toFixed(digits) : value.trim();
}

export async function retry<T>(attempts: number, run: () => Promise<T>): Promise<T> {
  let last: unknown;
  for (let attempt = 0; attempt < attempts; attempt++) {
    try {
      return await run();
    } catch (error) {
      last = error;
    }
  }
  throw last instanceof Error ? last : new Error(String(last));
}
"""

STATUSES = ["draft", "active", "suspended", "archived", "deleted"]
UNITS = ["kg", "m", "s", "item", "hour"]


def pascal(index: int) -> str:
    return f"Model{index:03d}"


def module_source(index: int, modules: int) -> str:
    name = pascal(index)
    lower = f"model{index:03d}"
    brand = f'"{lower}"'
    dependency = index - 1 if index > 0 else None
    second = index - 7 if index >= 7 else None
    status = STATUSES[index % len(STATUSES)]
    unit = UNITS[index % len(UNITS)]
    lines: list[str] = []
    emit = lines.append

    emit(f"// Generated domain module {index} of {modules}.")
    emit(
        'import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";'
    )
    emit('import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";')
    for other in (dependency, second):
        if other is not None:
            emit(
                f'import {{ {pascal(other)}Service, summarize{pascal(other)} }} from "./{pascal(other).lower()}";'
            )
            emit(f'import type {{ {pascal(other)} }} from "./{pascal(other).lower()}";')
    emit("")
    emit(f"export type {name}Status = {' | '.join(json.dumps(s) for s in STATUSES)};")
    emit("")
    emit(f"export interface {name}Line {{")
    emit("  readonly sku: string;")
    emit("  quantity: number;")
    emit(f'  unit: "{unit}" | "unit";')
    emit("  price?: { amount: number; currency: \"USD\" | \"EUR\" };")
    emit("}")
    emit("")
    emit(f"export interface {name} extends Entity<{brand}> {{")
    emit("  updatedAt?: number;")
    emit("  name: string;")
    emit(f"  status: {name}Status;")
    emit("  tags: string[];")
    emit(f"  lines: {name}Line[];")
    emit("  owner?: { name: string; email?: string };")
    if dependency is not None:
        emit(f"  parent?: {pascal(dependency)};")
    if second is not None:
        emit(f"  related: {pascal(second)}[];")
    emit("}")
    emit("")
    emit(f"export type {name}Event =")
    emit(f'  | {{ kind: "created"; item: {name} }}')
    emit(f'  | {{ kind: "renamed"; id: Id<{brand}>; from: string; to: string }}')
    emit(f'  | {{ kind: "moved"; id: Id<{brand}>; status: {name}Status }}')
    emit(f'  | {{ kind: "deleted"; id: Id<{brand}>; reason?: string }};')
    emit("")
    emit(f"export type {name}Events = {{")
    emit(f"  change: {name}Event;")
    emit("  error: { message: string; code: number };")
    emit("};")
    emit("")
    emit(f"export type {name}Numbers = KeysOfType<{name}Line, number>;")
    emit(f"export type Frozen{name} = DeepReadonly<{name}>;")
    emit("")
    emit(f"export function describe{name}Event(event: {name}Event): string {{")
    emit("  switch (event.kind) {")
    emit('    case "created":')
    emit("      return `created ${event.item.name}`;")
    emit('    case "renamed":')
    emit("      return `renamed ${event.from} -> ${event.to}`;")
    emit('    case "moved":')
    emit("      return `moved to ${event.status}`;")
    emit('    case "deleted":')
    emit('      return event.reason ? `deleted: ${event.reason}` : "deleted";')
    emit("    default: {")
    emit("      const unreachable: never = event;")
    emit("      return unreachable;")
    emit("    }")
    emit("  }")
    emit("}")
    emit("")
    emit(f"export function total{name}(item: {name}): number {{")
    emit("  let sum = 0;")
    emit("  for (const line of item.lines) {")
    emit("    const amount = line.price?.amount ?? 0;")
    emit('    sum += line.unit === "unit" ? amount : amount * line.quantity;')
    emit("  }")
    emit("  return sum;")
    emit("}")
    emit("")
    emit(f"export function summarize{name}(item: {name}): string {{")
    emit(f"  const head = pick(item, \"name\", \"status\");")
    emit(f"  const owner = item.owner?.email ?? item.owner?.name ?? \"nobody\";")
    parts = ["head.name", "head.status", "owner", f"format(total{name}(item), 3)"]
    if dependency is not None:
        parts.append(f'(item.parent ? summarize{pascal(dependency)}(item.parent) : "-")')
    if second is not None:
        parts.append(f'item.related.map(summarize{pascal(second)}).join(",")')
    emit(f"  return [{', '.join(parts)}].join(\" | \");")
    emit("}")
    emit("")
    emit(f"export class {name}Service extends Service<{name}, {brand}> {{")
    emit(f"  readonly events = new EventBus<{name}Events>();")
    if dependency is not None:
        emit(f"  private readonly parents?: {pascal(dependency)}Service;")
    emit("")
    emit(f"  constructor(repository = new MemoryRepository<{name}, {brand}>()) {{")
    emit("    super(repository);")
    emit("  }")
    emit("")
    emit(f"  validate(item: {name}): string[] {{")
    emit("    const problems: string[] = [];")
    emit("    if (item.name.trim().length === 0) {")
    emit('      problems.push("name is required");')
    emit("    }")
    emit("    for (const [index, line] of item.lines.entries()) {")
    emit("      if (line.quantity <= 0) {")
    emit("        problems.push(`line ${index} has no quantity`);")
    emit("      }")
    emit("    }")
    emit("    return problems;")
    emit("  }")
    emit("")
    emit(f"  rename(id: Id<{brand}>, to: string): Result<{name}> {{")
    emit(f"    const current = this.find((item) => item.id === id);")
    emit("    if (!current) {")
    emit('      return fail(`missing ${id}`);')
    emit("    }")
    emit(f"    const next: {name} = {{ ...current, name: to, updatedAt: current.createdAt + 1 }};")
    emit(f'    this.events.emit("change", {{ kind: "renamed", id, from: current.name, to }});')
    emit("    return this.repository.save(next);")
    emit("  }")
    emit("")
    emit(f"  move(id: Id<{brand}>, status: {name}Status): Result<{name}Status> {{")
    emit(f"    const saved = this.rename(id, status.toUpperCase());")
    emit("    return mapResult(saved, (item) => {")
    emit(f'      this.events.emit("change", {{ kind: "moved", id: item.id, status }});')
    emit("      return status;")
    emit("    });")
    emit("  }")
    emit("")
    emit(f"  apply(id: Id<{brand}>, patch: Patch<Pick<{name}, \"name\" | \"tags\">>): {name} | undefined {{")
    emit(f"    const current = this.find((item) => item.id === id);")
    emit("    if (current === undefined) {")
    emit("      return undefined;")
    emit("    }")
    emit("    return { ...current, name: patch.name ?? current.name, tags: current.tags };")
    emit("  }")
    emit("")
    emit(f"  byStatus(): Record<{name}Status, {name}[]> {{")
    emit("    return groupBy(this.repository.list(), (item) => item.status);")
    emit("  }")
    emit("")
    emit(f"  quantities<K extends {name}Numbers>(key: K): number[] {{")
    emit("    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));")
    emit("  }")
    emit("")
    emit(f"  async load(ids: readonly Id<{brand}>[]): Promise<{name}[]> {{")
    emit(f"    const found: {name}[] = [];")
    emit("    for (const id of ids) {")
    emit("      const item = await retry(3, async () => this.find((candidate) => candidate.id === id));")
    emit("      if (item) {")
    emit("        found.push(item);")
    emit("      }")
    emit("    }")
    emit("    return found;")
    emit("  }")
    if dependency is not None:
        emit("")
        emit(f"  parentNames(): string[] {{")
        emit("    return this.repository")
        emit("      .list((item) => item.parent !== undefined)")
        emit(f"      .map((item) => unwrapOr(ok(item.parent?.name ?? \"\"), \"\"));")
        emit("  }")
        emit("")
        emit(f"  linkedService(): {pascal(dependency)}Service {{")
        emit(f"    return this.parents ?? new {pascal(dependency)}Service();")
        emit("  }")
    emit("}")
    emit("")
    emit(f"export function make{name}(id: string, name: string): {name} {{")
    emit("  return {")
    emit(f"    id: id as Id<{brand}>,")
    emit("    createdAt: 0,")
    emit("    name,")
    emit(f'    status: "{status}",')
    emit("    tags: [],")
    emit(f'    lines: [{{ sku: `${{id}}-1`, quantity: {index + 1}, unit: "{unit}" }}],')
    if second is not None:
        emit("    related: [],")
    emit("  };")
    emit("}")
    emit("")
    emit(f"export const {lower}Defaults: Frozen{name} = make{name}(\"default-{index}\", \"Default {index}\");")
    emit(f"export const {lower}Label = summarize{name}(make{name}(\"label\", \"Label\"));")
    emit("")
    return "\n".join(lines)


def main_source(modules: int) -> str:
    lines: list[str] = []
    emit = lines.append
    for index in range(modules):
        name = pascal(index)
        emit(f'import {{ {name}Service, make{name}, describe{name}Event }} from "./{name.lower()}";')
    emit("")
    emit("export function run(): string[] {")
    emit("  const out: string[] = [];")
    for index in range(modules):
        name = pascal(index)
        service = f"service{index:03d}"
        emit(f"  const {service} = new {name}Service();")
        emit(f'  const created{index:03d} = {service}.create(make{name}("a{index}", "First {index}"));')
        emit(f"  if (created{index:03d}.ok) {{")
        emit(
            f'    out.push(describe{name}Event({{ kind: "created", item: created{index:03d}.value }}));'
        )
        emit(f'    {service}.move(created{index:03d}.value.id, "archived");')
        emit("  } else {")
        emit(f"    out.push(...created{index:03d}.error);")
        emit("  }")
    emit("  return out;")
    emit("}")
    emit("")
    emit("// A benchmark that stops checking must fail its diagnostic fingerprint.")
    emit('export const control: number = run().join("\\n");')
    emit("")
    return "\n".join(lines)


TSCONFIG = {
    "compilerOptions": {
        "target": "es2022",
        "module": "esnext",
        "moduleResolution": "bundler",
        "strict": True,
        "skipLibCheck": True,
        "noEmit": True,
    },
    "include": ["src/**/*.ts"],
}


def generate(out: pathlib.Path, modules: int) -> None:
    src = out / "src"
    if src.exists():
        shutil.rmtree(src)
    src.mkdir(parents=True)
    (src / "core.ts").write_text(CORE)
    for index in range(modules):
        (src / f"{pascal(index).lower()}.ts").write_text(module_source(index, modules))
    (src / "main.ts").write_text(main_source(modules))
    (out / "tsconfig.json").write_text(json.dumps(TSCONFIG, indent=2) + "\n")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--modules", type=int, default=DEFAULT_MODULES)
    parser.add_argument("--out", type=pathlib.Path, default=DEFAULT_OUT)
    args = parser.parse_args()
    generate(args.out, args.modules)


if __name__ == "__main__":
    main()
