// Generated domain module 191 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model190Service, summarizeModel190 } from "./model190";
import type { Model190 } from "./model190";
import { Model184Service, summarizeModel184 } from "./model184";
import type { Model184 } from "./model184";

export type Model191Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model191Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model191 extends Entity<"model191"> {
  updatedAt?: number;
  name: string;
  status: Model191Status;
  tags: string[];
  lines: Model191Line[];
  owner?: { name: string; email?: string };
  parent?: Model190;
  related: Model184[];
}

export type Model191Event =
  | { kind: "created"; item: Model191 }
  | { kind: "renamed"; id: Id<"model191">; from: string; to: string }
  | { kind: "moved"; id: Id<"model191">; status: Model191Status }
  | { kind: "deleted"; id: Id<"model191">; reason?: string };

export type Model191Events = {
  change: Model191Event;
  error: { message: string; code: number };
};

export type Model191Numbers = KeysOfType<Model191Line, number>;
export type FrozenModel191 = DeepReadonly<Model191>;

export function describeModel191Event(event: Model191Event): string {
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

export function totalModel191(item: Model191): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel191(item: Model191): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel191(item), 3), (item.parent ? summarizeModel190(item.parent) : "-"), item.related.map(summarizeModel184).join(",")].join(" | ");
}

export class Model191Service extends Service<Model191, "model191"> {
  readonly events = new EventBus<Model191Events>();
  private readonly parents?: Model190Service;

  constructor(repository = new MemoryRepository<Model191, "model191">()) {
    super(repository);
  }

  validate(item: Model191): string[] {
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

  rename(id: Id<"model191">, to: string): Result<Model191> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model191 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model191">, status: Model191Status): Result<Model191Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model191">, patch: Patch<Pick<Model191, "name" | "tags">>): Model191 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model191Status, Model191[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model191Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model191">[]): Promise<Model191[]> {
    const found: Model191[] = [];
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

  linkedService(): Model190Service {
    return this.parents ?? new Model190Service();
  }
}

export function makeModel191(id: string, name: string): Model191 {
  return {
    id: id as Id<"model191">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 192, unit: "m" }],
    related: [],
  };
}

export const model191Defaults: FrozenModel191 = makeModel191("default-191", "Default 191");
export const model191Label = summarizeModel191(makeModel191("label", "Label"));
