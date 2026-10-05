// Generated domain module 119 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model118Service, summarizeModel118 } from "./model118";
import type { Model118 } from "./model118";
import { Model112Service, summarizeModel112 } from "./model112";
import type { Model112 } from "./model112";

export type Model119Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model119Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model119 extends Entity<"model119"> {
  updatedAt?: number;
  name: string;
  status: Model119Status;
  tags: string[];
  lines: Model119Line[];
  owner?: { name: string; email?: string };
  parent?: Model118;
  related: Model112[];
}

export type Model119Event =
  | { kind: "created"; item: Model119 }
  | { kind: "renamed"; id: Id<"model119">; from: string; to: string }
  | { kind: "moved"; id: Id<"model119">; status: Model119Status }
  | { kind: "deleted"; id: Id<"model119">; reason?: string };

export type Model119Events = {
  change: Model119Event;
  error: { message: string; code: number };
};

export type Model119Numbers = KeysOfType<Model119Line, number>;
export type FrozenModel119 = DeepReadonly<Model119>;

export function describeModel119Event(event: Model119Event): string {
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

export function totalModel119(item: Model119): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel119(item: Model119): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel119(item), 3), (item.parent ? summarizeModel118(item.parent) : "-"), item.related.map(summarizeModel112).join(",")].join(" | ");
}

export class Model119Service extends Service<Model119, "model119"> {
  readonly events = new EventBus<Model119Events>();
  private readonly parents?: Model118Service;

  constructor(repository = new MemoryRepository<Model119, "model119">()) {
    super(repository);
  }

  validate(item: Model119): string[] {
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

  rename(id: Id<"model119">, to: string): Result<Model119> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model119 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model119">, status: Model119Status): Result<Model119Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model119">, patch: Patch<Pick<Model119, "name" | "tags">>): Model119 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model119Status, Model119[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model119Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model119">[]): Promise<Model119[]> {
    const found: Model119[] = [];
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

  linkedService(): Model118Service {
    return this.parents ?? new Model118Service();
  }
}

export function makeModel119(id: string, name: string): Model119 {
  return {
    id: id as Id<"model119">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 120, unit: "hour" }],
    related: [],
  };
}

export const model119Defaults: FrozenModel119 = makeModel119("default-119", "Default 119");
export const model119Label = summarizeModel119(makeModel119("label", "Label"));
