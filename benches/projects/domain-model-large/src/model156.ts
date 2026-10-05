// Generated domain module 156 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model155Service, summarizeModel155 } from "./model155";
import type { Model155 } from "./model155";
import { Model149Service, summarizeModel149 } from "./model149";
import type { Model149 } from "./model149";

export type Model156Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model156Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model156 extends Entity<"model156"> {
  updatedAt?: number;
  name: string;
  status: Model156Status;
  tags: string[];
  lines: Model156Line[];
  owner?: { name: string; email?: string };
  parent?: Model155;
  related: Model149[];
}

export type Model156Event =
  | { kind: "created"; item: Model156 }
  | { kind: "renamed"; id: Id<"model156">; from: string; to: string }
  | { kind: "moved"; id: Id<"model156">; status: Model156Status }
  | { kind: "deleted"; id: Id<"model156">; reason?: string };

export type Model156Events = {
  change: Model156Event;
  error: { message: string; code: number };
};

export type Model156Numbers = KeysOfType<Model156Line, number>;
export type FrozenModel156 = DeepReadonly<Model156>;

export function describeModel156Event(event: Model156Event): string {
  switch (event.kind) {
    case "created":
      return `created ${event.item.name}`;
    case "renamed":
      return `renamed ${event.from} -> ${event.to}`;
    case "moved":
      return `moved to ${event.status}`;
    case "deleted":
      return event.reason ? `deleted: ${event.reason}` : "deleted";
    default: {
      const unreachable: never = event;
      return unreachable;
    }
  }
}

export function totalModel156(item: Model156): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel156(item: Model156): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel156(item), 3), (item.parent ? summarizeModel155(item.parent) : "-"), item.related.map(summarizeModel149).join(",")].join(" | ");
}

export class Model156Service extends Service<Model156, "model156"> {
  readonly events = new EventBus<Model156Events>();
  private readonly parents?: Model155Service;

  constructor(repository = new MemoryRepository<Model156, "model156">()) {
    super(repository);
  }

  validate(item: Model156): string[] {
    const problems: string[] = [];
    if (item.name.trim().length === 0) {
      problems.push("name is required");
    }
    for (const [index, line] of item.lines.entries()) {
      if (line.quantity <= 0) {
        problems.push(`line ${index} has no quantity`);
      }
    }
    return problems;
  }

  rename(id: Id<"model156">, to: string): Result<Model156> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model156 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model156">, status: Model156Status): Result<Model156Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model156">, patch: Patch<Pick<Model156, "name" | "tags">>): Model156 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model156Status, Model156[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model156Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model156">[]): Promise<Model156[]> {
    const found: Model156[] = [];
    for (const id of ids) {
      const item = await retry(3, async () => this.find((candidate) => candidate.id === id));
      if (item) {
        found.push(item);
      }
    }
    return found;
  }

  parentNames(): string[] {
    return this.repository
      .list((item) => item.parent !== undefined)
      .map((item) => unwrapOr(ok(item.parent?.name ?? ""), ""));
  }

  linkedService(): Model155Service {
    return this.parents ?? new Model155Service();
  }
}

export function makeModel156(id: string, name: string): Model156 {
  return {
    id: id as Id<"model156">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 157, unit: "m" }],
    related: [],
  };
}

export const model156Defaults: FrozenModel156 = makeModel156("default-156", "Default 156");
export const model156Label = summarizeModel156(makeModel156("label", "Label"));
