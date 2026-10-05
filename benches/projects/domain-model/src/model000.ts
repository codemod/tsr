// Generated domain module 0 of 40.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";

export type Model000Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model000Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model000 extends Entity<"model000"> {
  updatedAt?: number;
  name: string;
  status: Model000Status;
  tags: string[];
  lines: Model000Line[];
  owner?: { name: string; email?: string };
}

export type Model000Event =
  | { kind: "created"; item: Model000 }
  | { kind: "renamed"; id: Id<"model000">; from: string; to: string }
  | { kind: "moved"; id: Id<"model000">; status: Model000Status }
  | { kind: "deleted"; id: Id<"model000">; reason?: string };

export type Model000Events = {
  change: Model000Event;
  error: { message: string; code: number };
};

export type Model000Numbers = KeysOfType<Model000Line, number>;
export type FrozenModel000 = DeepReadonly<Model000>;

export function describeModel000Event(event: Model000Event): string {
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

export function totalModel000(item: Model000): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel000(item: Model000): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel000(item), 3)].join(" | ");
}

export class Model000Service extends Service<Model000, "model000"> {
  readonly events = new EventBus<Model000Events>();

  constructor(repository = new MemoryRepository<Model000, "model000">()) {
    super(repository);
  }

  validate(item: Model000): string[] {
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

  rename(id: Id<"model000">, to: string): Result<Model000> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model000 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model000">, status: Model000Status): Result<Model000Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model000">, patch: Patch<Pick<Model000, "name" | "tags">>): Model000 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model000Status, Model000[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model000Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model000">[]): Promise<Model000[]> {
    const found: Model000[] = [];
    for (const id of ids) {
      const item = await retry(3, async () => this.find((candidate) => candidate.id === id));
      if (item) {
        found.push(item);
      }
    }
    return found;
  }
}

export function makeModel000(id: string, name: string): Model000 {
  return {
    id: id as Id<"model000">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 1, unit: "kg" }],
  };
}

export const model000Defaults: FrozenModel000 = makeModel000("default-0", "Default 0");
export const model000Label = summarizeModel000(makeModel000("label", "Label"));
