// Generated domain module 136 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model135Service, summarizeModel135 } from "./model135";
import type { Model135 } from "./model135";
import { Model129Service, summarizeModel129 } from "./model129";
import type { Model129 } from "./model129";

export type Model136Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model136Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model136 extends Entity<"model136"> {
  updatedAt?: number;
  name: string;
  status: Model136Status;
  tags: string[];
  lines: Model136Line[];
  owner?: { name: string; email?: string };
  parent?: Model135;
  related: Model129[];
}

export type Model136Event =
  | { kind: "created"; item: Model136 }
  | { kind: "renamed"; id: Id<"model136">; from: string; to: string }
  | { kind: "moved"; id: Id<"model136">; status: Model136Status }
  | { kind: "deleted"; id: Id<"model136">; reason?: string };

export type Model136Events = {
  change: Model136Event;
  error: { message: string; code: number };
};

export type Model136Numbers = KeysOfType<Model136Line, number>;
export type FrozenModel136 = DeepReadonly<Model136>;

export function describeModel136Event(event: Model136Event): string {
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

export function totalModel136(item: Model136): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel136(item: Model136): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel136(item), 3), (item.parent ? summarizeModel135(item.parent) : "-"), item.related.map(summarizeModel129).join(",")].join(" | ");
}

export class Model136Service extends Service<Model136, "model136"> {
  readonly events = new EventBus<Model136Events>();
  private readonly parents?: Model135Service;

  constructor(repository = new MemoryRepository<Model136, "model136">()) {
    super(repository);
  }

  validate(item: Model136): string[] {
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

  rename(id: Id<"model136">, to: string): Result<Model136> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model136 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model136">, status: Model136Status): Result<Model136Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model136">, patch: Patch<Pick<Model136, "name" | "tags">>): Model136 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model136Status, Model136[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model136Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model136">[]): Promise<Model136[]> {
    const found: Model136[] = [];
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

  linkedService(): Model135Service {
    return this.parents ?? new Model135Service();
  }
}

export function makeModel136(id: string, name: string): Model136 {
  return {
    id: id as Id<"model136">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 137, unit: "m" }],
    related: [],
  };
}

export const model136Defaults: FrozenModel136 = makeModel136("default-136", "Default 136");
export const model136Label = summarizeModel136(makeModel136("label", "Label"));
