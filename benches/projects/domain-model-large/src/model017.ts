// Generated domain module 17 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model016Service, summarizeModel016 } from "./model016";
import type { Model016 } from "./model016";
import { Model010Service, summarizeModel010 } from "./model010";
import type { Model010 } from "./model010";

export type Model017Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model017Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model017 extends Entity<"model017"> {
  updatedAt?: number;
  name: string;
  status: Model017Status;
  tags: string[];
  lines: Model017Line[];
  owner?: { name: string; email?: string };
  parent?: Model016;
  related: Model010[];
}

export type Model017Event =
  | { kind: "created"; item: Model017 }
  | { kind: "renamed"; id: Id<"model017">; from: string; to: string }
  | { kind: "moved"; id: Id<"model017">; status: Model017Status }
  | { kind: "deleted"; id: Id<"model017">; reason?: string };

export type Model017Events = {
  change: Model017Event;
  error: { message: string; code: number };
};

export type Model017Numbers = KeysOfType<Model017Line, number>;
export type FrozenModel017 = DeepReadonly<Model017>;

export function describeModel017Event(event: Model017Event): string {
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

export function totalModel017(item: Model017): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel017(item: Model017): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel017(item), 3), (item.parent ? summarizeModel016(item.parent) : "-"), item.related.map(summarizeModel010).join(",")].join(" | ");
}

export class Model017Service extends Service<Model017, "model017"> {
  readonly events = new EventBus<Model017Events>();
  private readonly parents?: Model016Service;

  constructor(repository = new MemoryRepository<Model017, "model017">()) {
    super(repository);
  }

  validate(item: Model017): string[] {
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

  rename(id: Id<"model017">, to: string): Result<Model017> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model017 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model017">, status: Model017Status): Result<Model017Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model017">, patch: Patch<Pick<Model017, "name" | "tags">>): Model017 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model017Status, Model017[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model017Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model017">[]): Promise<Model017[]> {
    const found: Model017[] = [];
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

  linkedService(): Model016Service {
    return this.parents ?? new Model016Service();
  }
}

export function makeModel017(id: string, name: string): Model017 {
  return {
    id: id as Id<"model017">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 18, unit: "s" }],
    related: [],
  };
}

export const model017Defaults: FrozenModel017 = makeModel017("default-17", "Default 17");
export const model017Label = summarizeModel017(makeModel017("label", "Label"));
