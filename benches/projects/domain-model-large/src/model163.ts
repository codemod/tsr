// Generated domain module 163 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model162Service, summarizeModel162 } from "./model162";
import type { Model162 } from "./model162";
import { Model156Service, summarizeModel156 } from "./model156";
import type { Model156 } from "./model156";

export type Model163Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model163Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model163 extends Entity<"model163"> {
  updatedAt?: number;
  name: string;
  status: Model163Status;
  tags: string[];
  lines: Model163Line[];
  owner?: { name: string; email?: string };
  parent?: Model162;
  related: Model156[];
}

export type Model163Event =
  | { kind: "created"; item: Model163 }
  | { kind: "renamed"; id: Id<"model163">; from: string; to: string }
  | { kind: "moved"; id: Id<"model163">; status: Model163Status }
  | { kind: "deleted"; id: Id<"model163">; reason?: string };

export type Model163Events = {
  change: Model163Event;
  error: { message: string; code: number };
};

export type Model163Numbers = KeysOfType<Model163Line, number>;
export type FrozenModel163 = DeepReadonly<Model163>;

export function describeModel163Event(event: Model163Event): string {
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

export function totalModel163(item: Model163): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel163(item: Model163): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel163(item), 3), (item.parent ? summarizeModel162(item.parent) : "-"), item.related.map(summarizeModel156).join(",")].join(" | ");
}

export class Model163Service extends Service<Model163, "model163"> {
  readonly events = new EventBus<Model163Events>();
  private readonly parents?: Model162Service;

  constructor(repository = new MemoryRepository<Model163, "model163">()) {
    super(repository);
  }

  validate(item: Model163): string[] {
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

  rename(id: Id<"model163">, to: string): Result<Model163> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model163 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model163">, status: Model163Status): Result<Model163Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model163">, patch: Patch<Pick<Model163, "name" | "tags">>): Model163 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model163Status, Model163[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model163Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model163">[]): Promise<Model163[]> {
    const found: Model163[] = [];
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

  linkedService(): Model162Service {
    return this.parents ?? new Model162Service();
  }
}

export function makeModel163(id: string, name: string): Model163 {
  return {
    id: id as Id<"model163">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 164, unit: "item" }],
    related: [],
  };
}

export const model163Defaults: FrozenModel163 = makeModel163("default-163", "Default 163");
export const model163Label = summarizeModel163(makeModel163("label", "Label"));
