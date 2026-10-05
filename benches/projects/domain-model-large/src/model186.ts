// Generated domain module 186 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model185Service, summarizeModel185 } from "./model185";
import type { Model185 } from "./model185";
import { Model179Service, summarizeModel179 } from "./model179";
import type { Model179 } from "./model179";

export type Model186Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model186Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model186 extends Entity<"model186"> {
  updatedAt?: number;
  name: string;
  status: Model186Status;
  tags: string[];
  lines: Model186Line[];
  owner?: { name: string; email?: string };
  parent?: Model185;
  related: Model179[];
}

export type Model186Event =
  | { kind: "created"; item: Model186 }
  | { kind: "renamed"; id: Id<"model186">; from: string; to: string }
  | { kind: "moved"; id: Id<"model186">; status: Model186Status }
  | { kind: "deleted"; id: Id<"model186">; reason?: string };

export type Model186Events = {
  change: Model186Event;
  error: { message: string; code: number };
};

export type Model186Numbers = KeysOfType<Model186Line, number>;
export type FrozenModel186 = DeepReadonly<Model186>;

export function describeModel186Event(event: Model186Event): string {
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

export function totalModel186(item: Model186): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel186(item: Model186): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel186(item), 3), (item.parent ? summarizeModel185(item.parent) : "-"), item.related.map(summarizeModel179).join(",")].join(" | ");
}

export class Model186Service extends Service<Model186, "model186"> {
  readonly events = new EventBus<Model186Events>();
  private readonly parents?: Model185Service;

  constructor(repository = new MemoryRepository<Model186, "model186">()) {
    super(repository);
  }

  validate(item: Model186): string[] {
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

  rename(id: Id<"model186">, to: string): Result<Model186> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model186 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model186">, status: Model186Status): Result<Model186Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model186">, patch: Patch<Pick<Model186, "name" | "tags">>): Model186 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model186Status, Model186[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model186Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model186">[]): Promise<Model186[]> {
    const found: Model186[] = [];
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

  linkedService(): Model185Service {
    return this.parents ?? new Model185Service();
  }
}

export function makeModel186(id: string, name: string): Model186 {
  return {
    id: id as Id<"model186">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 187, unit: "m" }],
    related: [],
  };
}

export const model186Defaults: FrozenModel186 = makeModel186("default-186", "Default 186");
export const model186Label = summarizeModel186(makeModel186("label", "Label"));
