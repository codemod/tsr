// Generated domain module 170 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model169Service, summarizeModel169 } from "./model169";
import type { Model169 } from "./model169";
import { Model163Service, summarizeModel163 } from "./model163";
import type { Model163 } from "./model163";

export type Model170Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model170Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model170 extends Entity<"model170"> {
  updatedAt?: number;
  name: string;
  status: Model170Status;
  tags: string[];
  lines: Model170Line[];
  owner?: { name: string; email?: string };
  parent?: Model169;
  related: Model163[];
}

export type Model170Event =
  | { kind: "created"; item: Model170 }
  | { kind: "renamed"; id: Id<"model170">; from: string; to: string }
  | { kind: "moved"; id: Id<"model170">; status: Model170Status }
  | { kind: "deleted"; id: Id<"model170">; reason?: string };

export type Model170Events = {
  change: Model170Event;
  error: { message: string; code: number };
};

export type Model170Numbers = KeysOfType<Model170Line, number>;
export type FrozenModel170 = DeepReadonly<Model170>;

export function describeModel170Event(event: Model170Event): string {
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

export function totalModel170(item: Model170): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel170(item: Model170): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel170(item), 3), (item.parent ? summarizeModel169(item.parent) : "-"), item.related.map(summarizeModel163).join(",")].join(" | ");
}

export class Model170Service extends Service<Model170, "model170"> {
  readonly events = new EventBus<Model170Events>();
  private readonly parents?: Model169Service;

  constructor(repository = new MemoryRepository<Model170, "model170">()) {
    super(repository);
  }

  validate(item: Model170): string[] {
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

  rename(id: Id<"model170">, to: string): Result<Model170> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model170 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model170">, status: Model170Status): Result<Model170Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model170">, patch: Patch<Pick<Model170, "name" | "tags">>): Model170 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model170Status, Model170[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model170Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model170">[]): Promise<Model170[]> {
    const found: Model170[] = [];
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

  linkedService(): Model169Service {
    return this.parents ?? new Model169Service();
  }
}

export function makeModel170(id: string, name: string): Model170 {
  return {
    id: id as Id<"model170">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 171, unit: "kg" }],
    related: [],
  };
}

export const model170Defaults: FrozenModel170 = makeModel170("default-170", "Default 170");
export const model170Label = summarizeModel170(makeModel170("label", "Label"));
