// Generated domain module 110 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model109Service, summarizeModel109 } from "./model109";
import type { Model109 } from "./model109";
import { Model103Service, summarizeModel103 } from "./model103";
import type { Model103 } from "./model103";

export type Model110Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model110Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model110 extends Entity<"model110"> {
  updatedAt?: number;
  name: string;
  status: Model110Status;
  tags: string[];
  lines: Model110Line[];
  owner?: { name: string; email?: string };
  parent?: Model109;
  related: Model103[];
}

export type Model110Event =
  | { kind: "created"; item: Model110 }
  | { kind: "renamed"; id: Id<"model110">; from: string; to: string }
  | { kind: "moved"; id: Id<"model110">; status: Model110Status }
  | { kind: "deleted"; id: Id<"model110">; reason?: string };

export type Model110Events = {
  change: Model110Event;
  error: { message: string; code: number };
};

export type Model110Numbers = KeysOfType<Model110Line, number>;
export type FrozenModel110 = DeepReadonly<Model110>;

export function describeModel110Event(event: Model110Event): string {
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

export function totalModel110(item: Model110): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel110(item: Model110): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel110(item), 3), (item.parent ? summarizeModel109(item.parent) : "-"), item.related.map(summarizeModel103).join(",")].join(" | ");
}

export class Model110Service extends Service<Model110, "model110"> {
  readonly events = new EventBus<Model110Events>();
  private readonly parents?: Model109Service;

  constructor(repository = new MemoryRepository<Model110, "model110">()) {
    super(repository);
  }

  validate(item: Model110): string[] {
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

  rename(id: Id<"model110">, to: string): Result<Model110> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model110 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model110">, status: Model110Status): Result<Model110Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model110">, patch: Patch<Pick<Model110, "name" | "tags">>): Model110 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model110Status, Model110[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model110Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model110">[]): Promise<Model110[]> {
    const found: Model110[] = [];
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

  linkedService(): Model109Service {
    return this.parents ?? new Model109Service();
  }
}

export function makeModel110(id: string, name: string): Model110 {
  return {
    id: id as Id<"model110">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 111, unit: "kg" }],
    related: [],
  };
}

export const model110Defaults: FrozenModel110 = makeModel110("default-110", "Default 110");
export const model110Label = summarizeModel110(makeModel110("label", "Label"));
