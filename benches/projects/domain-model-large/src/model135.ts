// Generated domain module 135 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model134Service, summarizeModel134 } from "./model134";
import type { Model134 } from "./model134";
import { Model128Service, summarizeModel128 } from "./model128";
import type { Model128 } from "./model128";

export type Model135Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model135Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model135 extends Entity<"model135"> {
  updatedAt?: number;
  name: string;
  status: Model135Status;
  tags: string[];
  lines: Model135Line[];
  owner?: { name: string; email?: string };
  parent?: Model134;
  related: Model128[];
}

export type Model135Event =
  | { kind: "created"; item: Model135 }
  | { kind: "renamed"; id: Id<"model135">; from: string; to: string }
  | { kind: "moved"; id: Id<"model135">; status: Model135Status }
  | { kind: "deleted"; id: Id<"model135">; reason?: string };

export type Model135Events = {
  change: Model135Event;
  error: { message: string; code: number };
};

export type Model135Numbers = KeysOfType<Model135Line, number>;
export type FrozenModel135 = DeepReadonly<Model135>;

export function describeModel135Event(event: Model135Event): string {
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

export function totalModel135(item: Model135): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel135(item: Model135): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel135(item), 3), (item.parent ? summarizeModel134(item.parent) : "-"), item.related.map(summarizeModel128).join(",")].join(" | ");
}

export class Model135Service extends Service<Model135, "model135"> {
  readonly events = new EventBus<Model135Events>();
  private readonly parents?: Model134Service;

  constructor(repository = new MemoryRepository<Model135, "model135">()) {
    super(repository);
  }

  validate(item: Model135): string[] {
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

  rename(id: Id<"model135">, to: string): Result<Model135> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model135 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model135">, status: Model135Status): Result<Model135Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model135">, patch: Patch<Pick<Model135, "name" | "tags">>): Model135 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model135Status, Model135[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model135Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model135">[]): Promise<Model135[]> {
    const found: Model135[] = [];
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

  linkedService(): Model134Service {
    return this.parents ?? new Model134Service();
  }
}

export function makeModel135(id: string, name: string): Model135 {
  return {
    id: id as Id<"model135">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 136, unit: "kg" }],
    related: [],
  };
}

export const model135Defaults: FrozenModel135 = makeModel135("default-135", "Default 135");
export const model135Label = summarizeModel135(makeModel135("label", "Label"));
