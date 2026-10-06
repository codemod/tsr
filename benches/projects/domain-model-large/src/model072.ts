// Generated domain module 72 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model071Service, summarizeModel071 } from "./model071";
import type { Model071 } from "./model071";
import { Model065Service, summarizeModel065 } from "./model065";
import type { Model065 } from "./model065";

export type Model072Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model072Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model072 extends Entity<"model072"> {
  updatedAt?: number;
  name: string;
  status: Model072Status;
  tags: string[];
  lines: Model072Line[];
  owner?: { name: string; email?: string };
  parent?: Model071;
  related: Model065[];
}

export type Model072Event =
  | { kind: "created"; item: Model072 }
  | { kind: "renamed"; id: Id<"model072">; from: string; to: string }
  | { kind: "moved"; id: Id<"model072">; status: Model072Status }
  | { kind: "deleted"; id: Id<"model072">; reason?: string };

export type Model072Events = {
  change: Model072Event;
  error: { message: string; code: number };
};

export type Model072Numbers = KeysOfType<Model072Line, number>;
export type FrozenModel072 = DeepReadonly<Model072>;

export function describeModel072Event(event: Model072Event): string {
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

export function totalModel072(item: Model072): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel072(item: Model072): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel072(item), 3), (item.parent ? summarizeModel071(item.parent) : "-"), item.related.map(summarizeModel065).join(",")].join(" | ");
}

export class Model072Service extends Service<Model072, "model072"> {
  readonly events = new EventBus<Model072Events>();
  private readonly parents?: Model071Service;

  constructor(repository = new MemoryRepository<Model072, "model072">()) {
    super(repository);
  }

  validate(item: Model072): string[] {
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

  rename(id: Id<"model072">, to: string): Result<Model072> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model072 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model072">, status: Model072Status): Result<Model072Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model072">, patch: Patch<Pick<Model072, "name" | "tags">>): Model072 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model072Status, Model072[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model072Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model072">[]): Promise<Model072[]> {
    const found: Model072[] = [];
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

  linkedService(): Model071Service {
    return this.parents ?? new Model071Service();
  }
}

export function makeModel072(id: string, name: string): Model072 {
  return {
    id: id as Id<"model072">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 73, unit: "s" }],
    related: [],
  };
}

export const model072Defaults: FrozenModel072 = makeModel072("default-72", "Default 72");
export const model072Label = summarizeModel072(makeModel072("label", "Label"));
