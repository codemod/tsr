// Generated domain module 39 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model038Service, summarizeModel038 } from "./model038";
import type { Model038 } from "./model038";
import { Model032Service, summarizeModel032 } from "./model032";
import type { Model032 } from "./model032";

export type Model039Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model039Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model039 extends Entity<"model039"> {
  updatedAt?: number;
  name: string;
  status: Model039Status;
  tags: string[];
  lines: Model039Line[];
  owner?: { name: string; email?: string };
  parent?: Model038;
  related: Model032[];
}

export type Model039Event =
  | { kind: "created"; item: Model039 }
  | { kind: "renamed"; id: Id<"model039">; from: string; to: string }
  | { kind: "moved"; id: Id<"model039">; status: Model039Status }
  | { kind: "deleted"; id: Id<"model039">; reason?: string };

export type Model039Events = {
  change: Model039Event;
  error: { message: string; code: number };
};

export type Model039Numbers = KeysOfType<Model039Line, number>;
export type FrozenModel039 = DeepReadonly<Model039>;

export function describeModel039Event(event: Model039Event): string {
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

export function totalModel039(item: Model039): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel039(item: Model039): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel039(item), 3), (item.parent ? summarizeModel038(item.parent) : "-"), item.related.map(summarizeModel032).join(",")].join(" | ");
}

export class Model039Service extends Service<Model039, "model039"> {
  readonly events = new EventBus<Model039Events>();
  private readonly parents?: Model038Service;

  constructor(repository = new MemoryRepository<Model039, "model039">()) {
    super(repository);
  }

  validate(item: Model039): string[] {
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

  rename(id: Id<"model039">, to: string): Result<Model039> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model039 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model039">, status: Model039Status): Result<Model039Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model039">, patch: Patch<Pick<Model039, "name" | "tags">>): Model039 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model039Status, Model039[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model039Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model039">[]): Promise<Model039[]> {
    const found: Model039[] = [];
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

  linkedService(): Model038Service {
    return this.parents ?? new Model038Service();
  }
}

export function makeModel039(id: string, name: string): Model039 {
  return {
    id: id as Id<"model039">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 40, unit: "hour" }],
    related: [],
  };
}

export const model039Defaults: FrozenModel039 = makeModel039("default-39", "Default 39");
export const model039Label = summarizeModel039(makeModel039("label", "Label"));
