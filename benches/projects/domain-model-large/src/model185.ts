// Generated domain module 185 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model184Service, summarizeModel184 } from "./model184";
import type { Model184 } from "./model184";
import { Model178Service, summarizeModel178 } from "./model178";
import type { Model178 } from "./model178";

export type Model185Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model185Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model185 extends Entity<"model185"> {
  updatedAt?: number;
  name: string;
  status: Model185Status;
  tags: string[];
  lines: Model185Line[];
  owner?: { name: string; email?: string };
  parent?: Model184;
  related: Model178[];
}

export type Model185Event =
  | { kind: "created"; item: Model185 }
  | { kind: "renamed"; id: Id<"model185">; from: string; to: string }
  | { kind: "moved"; id: Id<"model185">; status: Model185Status }
  | { kind: "deleted"; id: Id<"model185">; reason?: string };

export type Model185Events = {
  change: Model185Event;
  error: { message: string; code: number };
};

export type Model185Numbers = KeysOfType<Model185Line, number>;
export type FrozenModel185 = DeepReadonly<Model185>;

export function describeModel185Event(event: Model185Event): string {
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

export function totalModel185(item: Model185): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel185(item: Model185): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel185(item), 3), (item.parent ? summarizeModel184(item.parent) : "-"), item.related.map(summarizeModel178).join(",")].join(" | ");
}

export class Model185Service extends Service<Model185, "model185"> {
  readonly events = new EventBus<Model185Events>();
  private readonly parents?: Model184Service;

  constructor(repository = new MemoryRepository<Model185, "model185">()) {
    super(repository);
  }

  validate(item: Model185): string[] {
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

  rename(id: Id<"model185">, to: string): Result<Model185> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model185 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model185">, status: Model185Status): Result<Model185Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model185">, patch: Patch<Pick<Model185, "name" | "tags">>): Model185 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model185Status, Model185[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model185Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model185">[]): Promise<Model185[]> {
    const found: Model185[] = [];
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

  linkedService(): Model184Service {
    return this.parents ?? new Model184Service();
  }
}

export function makeModel185(id: string, name: string): Model185 {
  return {
    id: id as Id<"model185">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 186, unit: "kg" }],
    related: [],
  };
}

export const model185Defaults: FrozenModel185 = makeModel185("default-185", "Default 185");
export const model185Label = summarizeModel185(makeModel185("label", "Label"));
