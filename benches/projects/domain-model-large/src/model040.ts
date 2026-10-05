// Generated domain module 40 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model039Service, summarizeModel039 } from "./model039";
import type { Model039 } from "./model039";
import { Model033Service, summarizeModel033 } from "./model033";
import type { Model033 } from "./model033";

export type Model040Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model040Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model040 extends Entity<"model040"> {
  updatedAt?: number;
  name: string;
  status: Model040Status;
  tags: string[];
  lines: Model040Line[];
  owner?: { name: string; email?: string };
  parent?: Model039;
  related: Model033[];
}

export type Model040Event =
  | { kind: "created"; item: Model040 }
  | { kind: "renamed"; id: Id<"model040">; from: string; to: string }
  | { kind: "moved"; id: Id<"model040">; status: Model040Status }
  | { kind: "deleted"; id: Id<"model040">; reason?: string };

export type Model040Events = {
  change: Model040Event;
  error: { message: string; code: number };
};

export type Model040Numbers = KeysOfType<Model040Line, number>;
export type FrozenModel040 = DeepReadonly<Model040>;

export function describeModel040Event(event: Model040Event): string {
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

export function totalModel040(item: Model040): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel040(item: Model040): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel040(item), 3), (item.parent ? summarizeModel039(item.parent) : "-"), item.related.map(summarizeModel033).join(",")].join(" | ");
}

export class Model040Service extends Service<Model040, "model040"> {
  readonly events = new EventBus<Model040Events>();
  private readonly parents?: Model039Service;

  constructor(repository = new MemoryRepository<Model040, "model040">()) {
    super(repository);
  }

  validate(item: Model040): string[] {
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

  rename(id: Id<"model040">, to: string): Result<Model040> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model040 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model040">, status: Model040Status): Result<Model040Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model040">, patch: Patch<Pick<Model040, "name" | "tags">>): Model040 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model040Status, Model040[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model040Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model040">[]): Promise<Model040[]> {
    const found: Model040[] = [];
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

  linkedService(): Model039Service {
    return this.parents ?? new Model039Service();
  }
}

export function makeModel040(id: string, name: string): Model040 {
  return {
    id: id as Id<"model040">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 41, unit: "kg" }],
    related: [],
  };
}

export const model040Defaults: FrozenModel040 = makeModel040("default-40", "Default 40");
export const model040Label = summarizeModel040(makeModel040("label", "Label"));
