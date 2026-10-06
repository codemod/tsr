// Generated domain module 67 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model066Service, summarizeModel066 } from "./model066";
import type { Model066 } from "./model066";
import { Model060Service, summarizeModel060 } from "./model060";
import type { Model060 } from "./model060";

export type Model067Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model067Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model067 extends Entity<"model067"> {
  updatedAt?: number;
  name: string;
  status: Model067Status;
  tags: string[];
  lines: Model067Line[];
  owner?: { name: string; email?: string };
  parent?: Model066;
  related: Model060[];
}

export type Model067Event =
  | { kind: "created"; item: Model067 }
  | { kind: "renamed"; id: Id<"model067">; from: string; to: string }
  | { kind: "moved"; id: Id<"model067">; status: Model067Status }
  | { kind: "deleted"; id: Id<"model067">; reason?: string };

export type Model067Events = {
  change: Model067Event;
  error: { message: string; code: number };
};

export type Model067Numbers = KeysOfType<Model067Line, number>;
export type FrozenModel067 = DeepReadonly<Model067>;

export function describeModel067Event(event: Model067Event): string {
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

export function totalModel067(item: Model067): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel067(item: Model067): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel067(item), 3), (item.parent ? summarizeModel066(item.parent) : "-"), item.related.map(summarizeModel060).join(",")].join(" | ");
}

export class Model067Service extends Service<Model067, "model067"> {
  readonly events = new EventBus<Model067Events>();
  private readonly parents?: Model066Service;

  constructor(repository = new MemoryRepository<Model067, "model067">()) {
    super(repository);
  }

  validate(item: Model067): string[] {
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

  rename(id: Id<"model067">, to: string): Result<Model067> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model067 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model067">, status: Model067Status): Result<Model067Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model067">, patch: Patch<Pick<Model067, "name" | "tags">>): Model067 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model067Status, Model067[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model067Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model067">[]): Promise<Model067[]> {
    const found: Model067[] = [];
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

  linkedService(): Model066Service {
    return this.parents ?? new Model066Service();
  }
}

export function makeModel067(id: string, name: string): Model067 {
  return {
    id: id as Id<"model067">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 68, unit: "s" }],
    related: [],
  };
}

export const model067Defaults: FrozenModel067 = makeModel067("default-67", "Default 67");
export const model067Label = summarizeModel067(makeModel067("label", "Label"));
