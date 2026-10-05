// Generated domain module 89 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model088Service, summarizeModel088 } from "./model088";
import type { Model088 } from "./model088";
import { Model082Service, summarizeModel082 } from "./model082";
import type { Model082 } from "./model082";

export type Model089Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model089Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model089 extends Entity<"model089"> {
  updatedAt?: number;
  name: string;
  status: Model089Status;
  tags: string[];
  lines: Model089Line[];
  owner?: { name: string; email?: string };
  parent?: Model088;
  related: Model082[];
}

export type Model089Event =
  | { kind: "created"; item: Model089 }
  | { kind: "renamed"; id: Id<"model089">; from: string; to: string }
  | { kind: "moved"; id: Id<"model089">; status: Model089Status }
  | { kind: "deleted"; id: Id<"model089">; reason?: string };

export type Model089Events = {
  change: Model089Event;
  error: { message: string; code: number };
};

export type Model089Numbers = KeysOfType<Model089Line, number>;
export type FrozenModel089 = DeepReadonly<Model089>;

export function describeModel089Event(event: Model089Event): string {
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

export function totalModel089(item: Model089): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel089(item: Model089): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel089(item), 3), (item.parent ? summarizeModel088(item.parent) : "-"), item.related.map(summarizeModel082).join(",")].join(" | ");
}

export class Model089Service extends Service<Model089, "model089"> {
  readonly events = new EventBus<Model089Events>();
  private readonly parents?: Model088Service;

  constructor(repository = new MemoryRepository<Model089, "model089">()) {
    super(repository);
  }

  validate(item: Model089): string[] {
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

  rename(id: Id<"model089">, to: string): Result<Model089> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model089 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model089">, status: Model089Status): Result<Model089Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model089">, patch: Patch<Pick<Model089, "name" | "tags">>): Model089 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model089Status, Model089[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model089Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model089">[]): Promise<Model089[]> {
    const found: Model089[] = [];
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

  linkedService(): Model088Service {
    return this.parents ?? new Model088Service();
  }
}

export function makeModel089(id: string, name: string): Model089 {
  return {
    id: id as Id<"model089">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 90, unit: "hour" }],
    related: [],
  };
}

export const model089Defaults: FrozenModel089 = makeModel089("default-89", "Default 89");
export const model089Label = summarizeModel089(makeModel089("label", "Label"));
